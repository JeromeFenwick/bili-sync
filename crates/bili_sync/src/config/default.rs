use rand::seq::IndexedRandom;

/// 默认的 auth_token 实现，生成随机 16 位字符串
pub(super) fn default_auth_token() -> String {
    let byte_choices = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789!@#$%^&*()_+-=";
    let mut rng = rand::rng();
    (0..16)
        .map(|_| *(byte_choices.choose(&mut rng).expect("choose byte failed")) as char)
        .collect()
}

pub(crate) fn default_bind_address() -> String {
    "0.0.0.0:12345".to_string()
}

pub(super) fn default_time_format() -> String {
    "%Y-%m-%d".to_string()
}

pub fn default_favorite_path() -> String {
    "收藏夹/{{name}}".to_owned()
}

pub fn default_collection_path() -> String {
    "合集/{{name}}".to_owned()
}

pub fn default_submission_path() -> String {
    "投稿/{{name}}".to_owned()
}

pub(super) fn default_notify_new_videos() -> bool {
    false
}

pub(super) fn default_notify_daily_summary() -> bool {
    false
}

pub(super) fn default_notification_interval() -> u64 {
    5 // 默认5秒，建议范围3-10秒
}

pub(super) fn default_daily_summary_cron() -> String {
    "0 0 9 * * *".to_string() // 默认每天早上9点
}

pub(super) fn default_enable_notification_quiet_hours() -> bool {
    false
}

pub(super) fn default_quiet_hours_start() -> u8 {
    22 // 默认晚上10点
}

pub(super) fn default_quiet_hours_end() -> u8 {
    9 // 默认早上9点
}