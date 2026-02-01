use std::sync::LazyLock;

use super::NotificationQueue;

/// 全局通知队列实例
pub static NOTIFICATION_QUEUE: LazyLock<NotificationQueue> = LazyLock::new(NotificationQueue::new);

