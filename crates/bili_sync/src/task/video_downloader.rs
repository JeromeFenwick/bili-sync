use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use sea_orm::DatabaseConnection;
use serde::Serialize;
use tokio::sync::{OnceCell, watch};
use tokio_cron_scheduler::{Job, JobScheduler};

use crate::adapter::{VideoSource, VideoSourceEnum};
use crate::bilibili::{self, BiliClient, BiliError};
use crate::config::{ARGS, Config, TEMPLATE, Trigger, VersionedConfig};
use crate::utils::model::get_enabled_video_sources;
use crate::utils::notify::{error_and_notify, notify};
use crate::workflow::process_video_source;

static INSTANCE: OnceCell<DownloadTaskManager> = OnceCell::const_new();

/// 启动周期下载视频的任务
pub async fn video_downloader(connection: DatabaseConnection, bili_client: Arc<BiliClient>) -> Result<()> {
    let task_manager = DownloadTaskManager::init(connection, bili_client).await?;
    task_manager.start().await
}

pub struct DownloadTaskManager {
    sched: Arc<tokio::sync::Mutex<JobScheduler>>,
    cx: Arc<TaskContext>,
    shutdown_rx: watch::Receiver<Result<()>>,
}

#[derive(Serialize, Default, Clone, Copy, Debug)]
pub struct TaskStatus {
    is_running: bool,
    last_run: Option<chrono::DateTime<chrono::Local>>,
    last_finish: Option<chrono::DateTime<chrono::Local>>,
    next_run: Option<chrono::DateTime<chrono::Local>>,
}

struct TaskContext {
    connection: DatabaseConnection,
    bili_client: Arc<BiliClient>,
    running: tokio::sync::Mutex<()>,
    status_tx: watch::Sender<TaskStatus>,
    status_rx: watch::Receiver<TaskStatus>,
    video_task_id: tokio::sync::Mutex<Option<uuid::Uuid>>, // 存储当前视频下载任务的 UUID
    daily_summary_task_id: tokio::sync::Mutex<Option<uuid::Uuid>>, // 存储每日汇总任务的 UUID
}

impl DownloadTaskManager {
    /// 初始化 DownloadTaskManager 单例
    pub async fn init(
        connection: DatabaseConnection,
        bili_client: Arc<BiliClient>,
    ) -> Result<&'static DownloadTaskManager> {
        INSTANCE
            .get_or_try_init(|| DownloadTaskManager::new(connection, bili_client))
            .await
    }

    /// 获取 DownloadTaskManager 单例，未初始化时直接 panic
    pub fn get() -> &'static DownloadTaskManager {
        INSTANCE.get().expect("DownloadTaskManager is not initialized")
    }

    /// 订阅下载任务的状态更新
    pub fn subscribe(&self) -> watch::Receiver<TaskStatus> {
        self.cx.status_rx.clone()
    }

    /// 手动执行一次下载任务
    pub async fn download_once(&self) -> Result<()> {
        let _ = self
            .sched
            .lock()
            .await
            .add(Job::new_one_shot_async(
                Duration::from_secs(0),
                DownloadTaskManager::download_video_task(self.cx.clone()),
            )?)
            .await?;
        Ok(())
    }

    /// 启动任务调度器
    async fn start(&self) -> Result<()> {
        self.sched.lock().await.start().await?;
        let mut shutdown_rx = self.shutdown_rx.clone();
        shutdown_rx.changed().await?;
        self.sched.lock().await.shutdown().await.context("任务调度器关闭失败")?;
        if let Err(e) = &*shutdown_rx.borrow() {
            bail!("{:#}", e);
        }
        Ok(())
    }

    /// 私有的调度器构造函数
    async fn new(connection: DatabaseConnection, bili_client: Arc<BiliClient>) -> Result<Self> {
        let sched = Arc::new(tokio::sync::Mutex::new(JobScheduler::new().await?));
        let (status_tx, status_rx) = watch::channel(TaskStatus::default());
        let (running, video_task_id, daily_summary_task_id) = (
            tokio::sync::Mutex::new(()),
            tokio::sync::Mutex::new(None),
            tokio::sync::Mutex::new(None),
        );
        let cx = Arc::new(TaskContext {
            connection,
            bili_client,
            running,
            status_tx,
            status_rx,
            video_task_id,
            daily_summary_task_id,
        });
        // 读取初始配置
        let mut rx = VersionedConfig::get().subscribe();
        let initial_config = rx.borrow_and_update().clone();
        if ARGS.disable_credential_refresh {
            warn!("已禁用凭据检查与刷新任务，bili-sync 将不会自动检查刷新 Credential，需要用户自行维护");
        } else {
            // 初始化凭据检查与刷新任务，该任务必须成功，否则直接退出
            sched
                .lock()
                .await
                .add(Job::new_async_tz(
                    "0 0 1 * * *",
                    chrono::Local,
                    DownloadTaskManager::check_and_refresh_credential_task(cx.clone()),
                )?)
                .await?;
        }
        // 初始化并添加视频下载任务，将任务 ID 保存到 TaskManager 中
        let video_task_id = async {
            let job_run = DownloadTaskManager::download_video_task(cx.clone());
            let job = match &initial_config.interval {
                Trigger::Interval(interval) => Job::new_repeated_async(Duration::from_secs(*interval), job_run)?,
                Trigger::Cron(cron) => Job::new_async_tz(cron, chrono::Local, job_run)?,
            };
            Result::<_, anyhow::Error>::Ok(sched.lock().await.add(job).await?)
        }
        .await;
        let video_task_id = match video_task_id {
            Ok(id) => Some(id),
            Err(err) => {
                error_and_notify(
                    &initial_config,
                    &cx.bili_client,
                    format!("❌ 初始化视频下载任务失败 错误信息: {:#}", err),
                );
                None
            }
        };
        *cx.video_task_id.lock().await = video_task_id;
        // 发起一个一次性的任务，更新一下下次运行的时间
        if let Some(video_task_id) = video_task_id {
            sched
                .lock()
                .await
                .add(Job::new_one_shot_async(
                    Duration::from_secs(0),
                    DownloadTaskManager::refresh_next_run(video_task_id, cx.clone()),
                )?)
                .await?;
        }
        // 初始化每日汇总任务
        let daily_summary_task_id = crate::task::daily_summary::init_daily_summary_task(
            cx.connection.clone(),
            cx.bili_client.clone(),
            sched.clone(),
        )
        .await
        .context("初始化每日汇总任务失败")?;
        *cx.daily_summary_task_id.lock().await = Some(daily_summary_task_id);
        
        // 发起一个新任务，用来监听配置变更，动态更新视频下载任务
        let cx_clone = cx.clone();
        let sched_clone = sched.clone();
        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(Ok(()));
        tokio::spawn(async move {
            let update_task_result = async {
                while rx.changed().await.is_ok() {
                    let new_config = rx.borrow().clone();
                    let cx = cx_clone.clone();
                    let mut video_task_id = cx.video_task_id.lock().await;
                    if let Some(old_video_task_id) = *video_task_id {
                        // 这里必须成功，不然后面会重复添加任务
                        sched_clone
                            .lock()
                            .await
                            .remove(&old_video_task_id)
                            .await
                            .context("移除旧的视频下载任务失败")?;
                    }
                    let new_video_task_id = async {
                        let job_run = DownloadTaskManager::download_video_task(cx.clone());
                        let job = match &new_config.interval {
                            Trigger::Interval(interval) => {
                                Job::new_repeated_async(Duration::from_secs(*interval), job_run)?
                            }
                            Trigger::Cron(cron) => Job::new_async_tz(cron, chrono::Local, job_run)?,
                        };
                        Result::<_, anyhow::Error>::Ok(sched_clone.lock().await.add(job).await?)
                    }
                    .await;
                    let new_video_task_id = match new_video_task_id {
                        Ok(id) => Some(id),
                        Err(err) => {
                            error_and_notify(
                                &initial_config,
                                &cx.bili_client,
                                format!("❌ 重载视频下载任务失败 错误信息: {:#}", err),
                            );
                            None
                        }
                    };
                    *video_task_id = new_video_task_id;
                    if let Some(video_task_id) = new_video_task_id {
                        sched_clone
                            .lock()
                            .await
                            .add(Job::new_one_shot_async(
                                Duration::from_secs(0),
                                DownloadTaskManager::refresh_next_run(video_task_id, cx.clone()),
                            )?)
                            .await?;
                    }
                    
                    // 更新每日汇总任务
                    let mut daily_summary_task_id = cx.daily_summary_task_id.lock().await;
                    if let Some(old_daily_summary_task_id) = *daily_summary_task_id {
                        let _ = sched_clone
                            .lock()
                            .await
                            .remove(&old_daily_summary_task_id)
                            .await;
                    }
                    if new_config.notify_daily_summary {
                        match crate::task::daily_summary::init_daily_summary_task(
                            cx.connection.clone(),
                            cx.bili_client.clone(),
                            sched_clone.clone(),
                        )
                        .await
                        {
                            Ok(new_daily_summary_task_id) => {
                                *daily_summary_task_id = Some(new_daily_summary_task_id);
                            }
                            Err(e) => {
                                error_and_notify(
                                    &new_config,
                                    &cx.bili_client,
                                    format!("❌ 重载每日汇总任务失败 错误信息: {:#}", e),
                                );
                            }
                        }
                    } else {
                        *daily_summary_task_id = None;
                    }
                }
                Result::<(), anyhow::Error>::Ok(())
            }
            .await;
            // 如果执行正常，上面应该是永远不会退出的
            let _ = shutdown_tx.send(update_task_result);
        });
        Ok(Self { sched, cx, shutdown_rx })
    }

    fn check_and_refresh_credential_task(
        cx: Arc<TaskContext>,
    ) -> impl FnMut(uuid::Uuid, JobScheduler) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        move |_uuid, _l| {
            let cx = cx.clone();
            Box::pin(async move {
                let _lock = cx.running.lock().await;
                let config = VersionedConfig::get().read();
                info!("开始执行本轮凭据检查与刷新任务..");
                match check_and_refresh_credential(&cx.connection, &cx.bili_client, &config).await {
                    Ok(_) => info!("本轮凭据检查与刷新任务执行完毕"),
                    Err(e) => {
                        error_and_notify(
                            &config,
                            &cx.bili_client,
                            format!("❌ 凭据检查与刷新任务执行失败 错误信息: {:#}", e),
                        );
                    }
                }
            })
        }
    }

    fn refresh_next_run(
        video_task_id: uuid::Uuid,
        cx: Arc<TaskContext>,
    ) -> impl FnMut(uuid::Uuid, JobScheduler) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        move |_uuid, mut l| {
            let cx = cx.clone();
            Box::pin(async move {
                let old_status = *cx.status_rx.borrow();
                let next_run = l
                    .next_tick_for_job(video_task_id)
                    .await
                    .ok()
                    .flatten()
                    .map(|dt| dt.with_timezone(&chrono::Local));
                let _ = cx.status_tx.send(TaskStatus { next_run, ..old_status });
            })
        }
    }

    fn download_video_task(
        cx: Arc<TaskContext>,
    ) -> impl FnMut(uuid::Uuid, JobScheduler) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        move |uuid, mut l| {
            let cx = cx.clone();
            Box::pin(async move {
                let Ok(_lock) = cx.running.try_lock() else {
                    warn!("上一次视频下载任务尚未结束，跳过本次执行..");
                    return;
                };
                let _ = cx.status_tx.send(TaskStatus {
                    is_running: true,
                    last_run: Some(chrono::Local::now()),
                    last_finish: None,
                    next_run: None,
                });
                info!("开始执行本轮视频下载任务..");
                let mut config = VersionedConfig::get().snapshot();
                match download_video(&cx.connection, &cx.bili_client, &mut config).await {
                    Ok(_) => info!("本轮视频下载任务执行完毕"),
                    Err(e) => {
                        error_and_notify(
                            &config,
                            &cx.bili_client,
                            format!("❌ 视频下载任务执行失败 错误信息: {:#}", e),
                        );
                    }
                }
                // 注意此处尽量从 updating 中读取 uuid，因为当前任务可能是不存在 next_tick 的 oneshot 任务
                let task_uuid = (*cx.video_task_id.lock().await).unwrap_or(uuid);
                let next_run = l
                    .next_tick_for_job(task_uuid)
                    .await
                    .ok()
                    .flatten()
                    .map(|dt| dt.with_timezone(&chrono::Local));
                let last_status = *cx.status_rx.borrow();
                let _ = cx.status_tx.send(TaskStatus {
                    is_running: false,
                    last_run: last_status.last_run,
                    last_finish: Some(chrono::Local::now()),
                    next_run,
                });
            })
        }
    }
}

async fn check_and_refresh_credential(
    connection: &DatabaseConnection,
    bili_client: &BiliClient,
    config: &Config,
) -> Result<()> {
    match bili_client
        .check_refresh(&config.credential)
        .await
        .context("检查刷新 Credential 失败")?
    {
        None => {
            info!("Credential 无需刷新");
        }
        Some(new_credential) => {
            VersionedConfig::get()
                .update_credential(new_credential, connection)
                .await
                .context("新 Credential 持久化失败")?;
            info!("Credential 已刷新并保存");
            // 通知用户凭据已刷新
            let config = VersionedConfig::get().read();
            notify(
                &config,
                bili_client,
                "✅ 凭据已刷新 Credential 已自动刷新并保存，系统将继续正常运行。".to_string(),
            );
        }
    }
    Ok(())
}

async fn download_video(
    connection: &DatabaseConnection,
    bili_client: &BiliClient,
    config: &mut Arc<Config>,
) -> Result<()> {
    config.check().context("配置检查失败")?;
    let mixin_key = bili_client
        .wbi_img(&config.credential)
        .await
        .context("获取 wbi_img 失败")?
        .into_mixin_key()
        .context("解析 mixin key 失败")?;
    bilibili::set_global_mixin_key(mixin_key);
    let template = TEMPLATE.snapshot();
    let bili_client = bili_client.snapshot()?;
    let video_sources = get_enabled_video_sources(connection)
        .await
        .context("获取视频源列表失败")?;
    if video_sources.is_empty() {
        let msg = "⚠️ 没有可用的视频源 所有视频源均未启用，请检查视频源配置。";
        notify(config, &bili_client, msg.to_string());
        bail!("没有可用的视频源");
    }
    
    // 统计待扫描的视频源数量（总计）
    let mut total_collections = 0;
    let mut total_favorites = 0;
    let mut total_submissions = 0;
    let mut total_watch_later = 0;
    for source in &video_sources {
        match source {
            VideoSourceEnum::Collection(_) => total_collections += 1,
            VideoSourceEnum::Favorite(_) => total_favorites += 1,
            VideoSourceEnum::Submission(_) => total_submissions += 1,
            VideoSourceEnum::WatchLater(_) => total_watch_later += 1,
        }
    }
    
    // 统计扫描成功的数量
    let mut succeeded_collections = 0;
    let mut succeeded_favorites = 0;
    let mut succeeded_submissions = 0;
    let mut succeeded_watch_later = 0;
    
    // 记录因风控未扫描的视频源数量
    let mut risk_control_collections = 0;
    let mut risk_control_favorites = 0;
    let mut risk_control_submissions = 0;
    let mut risk_control_watch_later = 0;
    
    // 记录是否因风控中断
    let mut risk_control_triggered = false;
    let mut risk_control_source_type: Option<&str> = None;
    
    // 直接消费 video_sources，记录每个视频源的类型以便统计
    let mut remaining_sources: Vec<&str> = Vec::new();
    for video_source in &video_sources {
        let source_type = match video_source {
            VideoSourceEnum::Collection(_) => "collection",
            VideoSourceEnum::Favorite(_) => "favorite",
            VideoSourceEnum::Submission(_) => "submission",
            VideoSourceEnum::WatchLater(_) => "watch_later",
        };
        remaining_sources.push(source_type);
    }
    
    // 遍历并处理视频源
    for (index, video_source) in video_sources.into_iter().enumerate() {
        let display_name = video_source.display_name();
        let source_type = match &video_source {
            VideoSourceEnum::Collection(_) => "collection",
            VideoSourceEnum::Favorite(_) => "favorite",
            VideoSourceEnum::Submission(_) => "submission",
            VideoSourceEnum::WatchLater(_) => "watch_later",
        };
        
        if let Err(e) = process_video_source(video_source, &bili_client, connection, &template, config).await {
            // 检查是否是风控相关错误（使用 downcast_ref 避免消费错误）
            if let Some(bili_err) = e.downcast_ref::<BiliError>() 
                && bili_err.is_risk_control_related()
            {
                warn!("检测到风控，终止此轮视频下载任务 处理 {} 时触发风控: {:#}", display_name, e);
                risk_control_triggered = true;
                risk_control_source_type = Some(source_type);
                // 记录当前和后续未扫描的视频源
                for remaining_type in remaining_sources.iter().skip(index) {
                    match *remaining_type {
                        "collection" => risk_control_collections += 1,
                        "favorite" => risk_control_favorites += 1,
                        "submission" => risk_control_submissions += 1,
                        "watch_later" => risk_control_watch_later += 1,
                        _ => {}
                    }
                }
                break;
            }
            // 其他错误正常通知
            error_and_notify(
                config,
                &bili_client,
                format!("❌ 处理 {} 失败 错误信息: {:#} 已跳过该视频源", display_name, e),
            );
        } else {
            // 处理成功，根据类型增加计数
            match source_type {
                "collection" => succeeded_collections += 1,
                "favorite" => succeeded_favorites += 1,
                "submission" => succeeded_submissions += 1,
                "watch_later" => succeeded_watch_later += 1,
                _ => {}
            }
        }
    }
    
    // 输出统计信息
    let mut stats_parts = Vec::new();
    
    // 合集统计
    if total_collections > 0 {
        if risk_control_collections > 0 {
            stats_parts.push(format!("合集: {} / {} - 待扫描: {}", 
                succeeded_collections, total_collections, risk_control_collections));
        } else {
            stats_parts.push(format!("合集: {} / {}", succeeded_collections, total_collections));
        }
    }
    
    // 收藏夹统计
    if total_favorites > 0 {
        if risk_control_favorites > 0 {
            stats_parts.push(format!("收藏夹: {} / {} - 待扫描: {}", 
                succeeded_favorites, total_favorites, risk_control_favorites));
        } else {
            stats_parts.push(format!("收藏夹: {} / {}", succeeded_favorites, total_favorites));
        }
    }
    
    // 投稿统计
    if total_submissions > 0 {
        if risk_control_submissions > 0 {
            stats_parts.push(format!("投稿: {} / {} - 待扫描: {}", 
                succeeded_submissions, total_submissions, risk_control_submissions));
        } else {
            stats_parts.push(format!("投稿: {} / {}", succeeded_submissions, total_submissions));
        }
    }
    
    // 稍后再看统计
    if total_watch_later > 0 {
        if risk_control_watch_later > 0 {
            stats_parts.push(format!("稍后再看: {} / {} - 待扫描: {}", 
                succeeded_watch_later, total_watch_later, risk_control_watch_later));
        } else {
            stats_parts.push(format!("稍后再看: {} / {}", succeeded_watch_later, total_watch_later));
        }
    }
    
    let stats_message = format!("视频源扫描统计 - {}", stats_parts.join(" | "));
    info!("{}", stats_message);
    
    // 发送统计通知（静默时间段检查在 NotificationQueue 中统一处理）
    notify(config, &bili_client, stats_message);
    
    Ok(())
}
