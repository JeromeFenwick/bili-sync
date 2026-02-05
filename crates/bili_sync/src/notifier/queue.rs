use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use chrono::Timelike;
use tokio::sync::mpsc;
use tokio::time::sleep;
use tracing::{error, info, warn};

use crate::config::VersionedConfig;

use super::Notifier;

/// 消息队列，用于控制通知发送频率
pub struct NotificationQueue {
    sender: mpsc::UnboundedSender<NotificationMessage>,
}

#[derive(Clone)]
pub struct NotificationMessage {
    pub notifiers: Arc<Vec<Notifier>>,
    pub message: String,
    pub client: reqwest::Client,
    pub created_at: chrono::DateTime<chrono::Local>,
}

impl NotificationQueue {
    /// 创建新的消息队列
    pub fn new() -> Self {
        let (sender, mut receiver) = mpsc::unbounded_channel::<NotificationMessage>();
        
        // 启动后台任务处理消息队列
        let sender_for_delay = sender.clone();
        tokio::spawn(async move {
            while let Some(msg) = receiver.recv().await {
                // 检查静默时间段
                let config = VersionedConfig::get().read();
                let mut should_delay = false;
                
                if config.enable_notification_quiet_hours {
                    let now = chrono::Local::now();
                    let hour = now.hour() as u8;
                    let start_hour = config.quiet_hours_start;
                    let end_hour = config.quiet_hours_end;
                    
                    // 判断是否在静默时间段内
                    let is_quiet_time = if start_hour > end_hour {
                        // 跨天的情况，例如 22:00-09:00
                        hour >= start_hour || hour < end_hour
                    } else {
                        // 不跨天的情况，例如 22:00-23:00
                        hour >= start_hour && hour < end_hour
                    };
                    
                    if is_quiet_time {
                        // 计算到静默结束时间的延迟时间
                        let target_time = if start_hour > end_hour {
                            // 跨天的情况
                            if hour >= start_hour {
                                // 如果是开始时间之后，目标时间是明天的结束时间
                                now.date_naive()
                                    .succ_opt()
                                    .unwrap_or(now.date_naive())
                                    .and_hms_opt(end_hour as u32, 0, 0)
                                    .unwrap()
                                    .and_local_timezone(chrono::Local)
                                    .unwrap()
                            } else {
                                // 如果是结束时间之前，目标时间是今天的结束时间
                                now.date_naive()
                                    .and_hms_opt(end_hour as u32, 0, 0)
                                    .unwrap()
                                    .and_local_timezone(chrono::Local)
                                    .unwrap()
                            }
                        } else {
                            // 不跨天的情况，目标时间是今天的结束时间
                            now.date_naive()
                                .and_hms_opt(end_hour as u32, 0, 0)
                                .unwrap()
                                .and_local_timezone(chrono::Local)
                                .unwrap()
                        };
                        
                        let delay = target_time.signed_duration_since(now);
                        if delay.num_seconds() > 0 {
                            info!("当前时间在静默时间段内（{}:00-{}:00），延迟到 {}:00 发送通知（延迟 {} 秒）", 
                                start_hour, end_hour, end_hour, delay.num_seconds());
                            // 延迟后重新入队到主队列，以遵循队列间隔配置
                            let msg_clone = msg.clone();
                            let sender_for_delay_clone = sender_for_delay.clone();
                            tokio::spawn(async move {
                                tokio::time::sleep(Duration::from_secs(delay.num_seconds() as u64)).await;
                                // 延迟后重新入队到主队列，这样会遵循队列间隔配置
                                if let Err(e) = sender_for_delay_clone.send(msg_clone) {
                                    error!("延迟发送后重新入队失败: {:#}", e);
                                }
                            });
                            // 继续处理下一条消息，不等待延迟发送完成
                            should_delay = true;
                        }
                    }
                }
                
                if !should_delay {
                    // 不在静默时间段，立即发送
                    info!("开始发送通知消息（共 {} 个通知器）", msg.notifiers.len());
                    match Self::send_notification(&msg).await {
                        Ok(_) => {
                            info!("通知消息发送成功");
                        }
                        Err(e) => {
                            error!("发送通知失败: {:#}", e);
                        }
                    }
                }
                
                // 从配置中读取等待时间（默认5秒）
                let interval = VersionedConfig::get()
                    .read()
                    .notification_interval
                    .max(1) // 至少1秒
                    .min(60); // 最多60秒，避免过长
                sleep(Duration::from_secs(interval)).await;
            }
        });
        
        Self { sender }
    }
    
    /// 发送通知（实际执行）
    async fn send_notification(msg: &NotificationMessage) -> Result<()> {
        let mut success_count = 0;
        let mut fail_count = 0;
        
        // 获取发送时间
        let sent_at = chrono::Local::now();
        let created_at = msg.created_at;
        
        for (index, notifier) in msg.notifiers.iter().enumerate() {
            let notifier_type = match notifier {
                Notifier::Telegram { .. } => "Telegram",
                Notifier::Webhook { .. } => "Webhook",
            };
            
            // 统一使用原始消息和时间参数，让每个通知器自己决定如何显示时间
            let result = notifier.notify_with_time(&msg.client, &msg.message, Some(created_at), Some(sent_at)).await;
            
            match result {
                Ok(_) => {
                    success_count += 1;
                    info!("通知器 #{} ({}) 发送成功", index + 1, notifier_type);
                }
                Err(e) => {
                    fail_count += 1;
                    error!("通知器 #{} ({}) 发送失败: {:#}", index + 1, notifier_type, e);
                    // 继续发送其他通知器，不因一个失败而停止
                }
            }
        }
        
        if fail_count > 0 {
            warn!("通知发送完成: {} 成功, {} 失败", success_count, fail_count);
            if success_count == 0 {
                anyhow::bail!("所有通知器发送失败");
            }
        } else {
            info!("所有通知器发送成功");
        }
        
        Ok(())
    }
    
    /// 将消息加入队列
    pub fn enqueue(&self, msg: NotificationMessage) -> Result<()> {
        self.sender.send(msg)?;
        Ok(())
    }
}

impl Default for NotificationQueue {
    fn default() -> Self {
        Self::new()
    }
}

