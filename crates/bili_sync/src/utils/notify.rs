use crate::bilibili::BiliClient;
use crate::config::Config;
use crate::notifier::{NotifierAllExt, NOTIFICATION_QUEUE};

pub fn error_and_notify(config: &Config, bili_client: &BiliClient, msg: String) {
    error!("{msg}");
    // 使用消息队列发送，以便统一处理静默时间段
    notify(config, bili_client, msg);
}

/// 发送通知消息（使用消息队列）
pub fn notify(config: &Config, bili_client: &BiliClient, msg: String) {
    if let Some(notifiers) = &config.notifiers
        && !notifiers.is_empty()
    {
        let (notifiers, inner_client) = (notifiers.clone(), bili_client.inner_client().clone());
        let _ = notifiers.notify_all_queued(&NOTIFICATION_QUEUE, inner_client, msg);
    }
}
