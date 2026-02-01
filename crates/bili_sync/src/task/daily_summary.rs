use std::sync::Arc;

use anyhow::{Context, Result};
use sea_orm::DatabaseConnection;
use sea_orm::entity::prelude::*;
use tokio_cron_scheduler::{Job, JobScheduler};

use crate::bilibili::BiliClient;
use crate::config::VersionedConfig;
use crate::notifier::{NotifierAllExt, NOTIFICATION_QUEUE};
use crate::utils::model::get_enabled_video_sources;
use crate::utils::status::VideoStatus;
use bili_sync_entity::{video, favorite, collection, submission};
use sea_orm::Condition;

/// 初始化每日汇总任务，返回任务 ID
pub async fn init_daily_summary_task(
    connection: DatabaseConnection,
    bili_client: Arc<BiliClient>,
    sched: Arc<tokio::sync::Mutex<JobScheduler>>,
) -> Result<uuid::Uuid> {
    let config = VersionedConfig::get().read();
    let cron = config.daily_summary_cron.clone();
    
    let job = Job::new_async_tz(
        &cron,
        chrono::Local,
        move |_uuid, _l| {
            let connection = connection.clone();
            let bili_client = bili_client.clone();
            Box::pin(async move {
                let config = VersionedConfig::get().read();
                if !config.notify_daily_summary {
                    return;
                }
                
                if let Some(notifiers) = &config.notifiers
                    && !notifiers.is_empty()
                {
                    match generate_daily_summary(&connection).await {
                        Ok(summary) => {
                            let client = bili_client.inner_client().clone();
                            let _ = notifiers.notify_all_queued(
                                &NOTIFICATION_QUEUE,
                                client,
                                summary,
                            );
                        }
                        Err(e) => {
                            tracing::error!("生成每日汇总失败: {:#}", e);
                        }
                    }
                }
            })
        },
    )?;
    
    let task_id = sched.lock().await.add(job).await?;
    Ok(task_id)
}

/// 生成每日汇总消息
async fn generate_daily_summary(connection: &DatabaseConnection) -> Result<String> {
    // 获取所有视频源
    let video_sources = get_enabled_video_sources(connection)
        .await
        .context("获取视频源列表失败")?;
    
    // 统计各类视频数量
    let total_videos = video::Entity::find()
        .count(connection)
        .await?;
    
    let succeeded_videos = video::Entity::find()
        .filter(VideoStatus::query_builder().succeeded())
        .count(connection)
        .await?;
    
    let failed_videos = video::Entity::find()
        .filter(VideoStatus::query_builder().failed())
        .filter(video::Column::Valid.eq(true))
        .count(connection)
        .await?;
    
    // 等待中的视频：should_download=true 且 is_paid_video=false 且所有任务状态都是未开始
    let waiting_videos = video::Entity::find()
        .filter(
            Condition::all()
                .add(VideoStatus::query_builder().waiting())
                .add(video::Column::ShouldDownload.eq(true))
                .add(video::Column::IsPaidVideo.eq(false))
        )
        .count(connection)
        .await?;
    
    // 失效视频：should_download=false 且 is_paid_video=false
    let skipped_videos = video::Entity::find()
        .filter(
            Condition::all()
                .add(video::Column::ShouldDownload.eq(false))
                .add(video::Column::IsPaidVideo.eq(false))
        )
        .count(connection)
        .await?;
    
    // 收费视频：is_paid_video=true
    let paid_videos = video::Entity::find()
        .filter(video::Column::IsPaidVideo.eq(true))
        .count(connection)
        .await?;
    
    // 统计各类视频源数量（统计启用的源个数，不是视频个数）
    let favorite_count = favorite::Entity::find()
        .filter(favorite::Column::Enabled.eq(true))
        .count(connection)
        .await?;
    
    let collection_count = collection::Entity::find()
        .filter(collection::Column::Enabled.eq(true))
        .count(connection)
        .await?;
    
    let submission_count = submission::Entity::find()
        .filter(submission::Column::Enabled.eq(true))
        .count(connection)
        .await?;
    
    // 生成汇总消息
    let summary = format!(
        "📊 BiliSync 每日汇总 | 📹 视频总数: {} | ✅ 成功: {} | ❌ 失败: {} | ⏳ 等待: {} | 🔄 失效: {} | 💰 收费: {} | 📚 视频源: 收藏夹 {} 合集 {} UP投稿 {} 总计 {}",
        total_videos,
        succeeded_videos,
        failed_videos,
        waiting_videos,
        skipped_videos,
        paid_videos,
        favorite_count,
        collection_count,
        submission_count,
        video_sources.len()
    );
    
    Ok(summary)
}

