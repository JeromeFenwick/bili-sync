mod queue;
mod global;

use anyhow::Result;
use futures::future;
use reqwest::header;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};
use tracing::info;

use crate::config::TEMPLATE;

pub use queue::NotificationQueue;
pub use global::NOTIFICATION_QUEUE;

/// 全局消息缓存：按通知器维度缓存最近一次发送的“逻辑消息内容”
static LAST_MESSAGES: LazyLock<Mutex<HashMap<String, String>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum Notifier {
    Telegram {
        bot_token: String,
        chat_id: String,
    },
    Webhook {
        url: String,
        template: Option<String>,
        #[serde(skip)]
        // 一个内部辅助字段，用于决定是否强制渲染当前模板，在测试时使用
        ignore_cache: Option<()>,
    },
}

fn notifier_cache_key(notifier: &Notifier) -> String {
    match notifier {
        Notifier::Telegram { bot_token, chat_id } => {
            format!("telegram:{}:{}", bot_token, chat_id)
        }
        Notifier::Webhook { url, .. } => format!("webhook:{}", url),
    }
}

/// 归一化消息内容用于去重。
/// 这里直接使用业务侧传入的原始 message，不包含后续追加的时间信息，
/// 这样即使只是生成时间 / 推送时间不同，也会被视为“同一条消息”而被去重。
fn normalize_message_for_cache(_notifier: &Notifier, message: &str) -> String {
    message.trim().to_string()
}

pub fn webhook_template_key(url: &str) -> String {
    format!("payload_{}", url)
}

pub fn webhook_template_content(template: &Option<String>) -> &str {
    template
        .as_deref()
        .filter(|t| !t.trim().is_empty())
        .unwrap_or(r#"{"text": "{{{message}}}", "created_at": "{{created_at}}", "sent_at": "{{sent_at}}"}"#)
}

pub trait NotifierAllExt {
    async fn notify_all(&self, client: &reqwest::Client, message: &str) -> Result<()>;
    fn notify_all_queued(&self, queue: &NotificationQueue, client: reqwest::Client, message: String) -> Result<()>;
}

impl NotifierAllExt for Vec<Notifier> {
    async fn notify_all(&self, client: &reqwest::Client, message: &str) -> Result<()> {
        future::join_all(self.iter().map(|notifier| notifier.notify(client, message))).await;
        Ok(())
    }
    
    fn notify_all_queued(&self, queue: &NotificationQueue, client: reqwest::Client, message: String) -> Result<()> {
        queue.enqueue(queue::NotificationMessage {
            notifiers: Arc::new(self.clone()),
            message,
            client,
            created_at: chrono::Local::now(),
        })
    }
}

impl Notifier {
    /// 普通通知（走消息去重）
    pub async fn notify(&self, client: &reqwest::Client, message: &str) -> Result<()> {
        self.notify_internal(client, message, None, None, false).await
    }
    
    /// 携带时间信息的通知（走消息去重）
    pub async fn notify_with_time(
        &self,
        client: &reqwest::Client,
        message: &str,
        created_at: Option<chrono::DateTime<chrono::Local>>,
        sent_at: Option<chrono::DateTime<chrono::Local>>,
    ) -> Result<()> {
        self.notify_internal(client, message, created_at, sent_at, false).await
    }

    /// 强制发送通知，不走消息去重逻辑（用于测试通知）
    pub async fn notify_without_cache(
        &self,
        client: &reqwest::Client,
        message: &str,
    ) -> Result<()> {
        self.notify_internal(client, message, None, None, true).await
    }

    async fn notify_internal(
        &self,
        client: &reqwest::Client,
        message: &str,
        created_at: Option<chrono::DateTime<chrono::Local>>,
        sent_at: Option<chrono::DateTime<chrono::Local>>,
        bypass_cache: bool,
    ) -> Result<()> {
        // 消息去重：同一个通知器，如果本次“逻辑消息内容”和上次完全一致，则跳过发送
        if !bypass_cache {
            let key = notifier_cache_key(self);
            let normalized = normalize_message_for_cache(self, message);
            let mut cache = LAST_MESSAGES
                .lock()
                .expect("LAST_MESSAGES mutex poisoned");

            if let Some(last) = cache.get(&key) {
                if last == &normalized {
                    info!("通知内容与上次完全相同，已跳过发送（key = {}）", key);
                    return Ok(());
                }
            }

            cache.insert(key, normalized);
        }

        match self {
            Notifier::Telegram { bot_token, chat_id } => {
                // 如果有时间信息，添加到消息末尾
                let final_message = if let (Some(created_at), Some(sent_at)) = (created_at, sent_at) {
                    let created_time = created_at.format("%Y-%m-%d %H:%M:%S").to_string();
                    let sent_time = sent_at.format("%Y-%m-%d %H:%M:%S").to_string();
                    format!("{}\n\n⌛️ 生成时间: {}\n⌛️ 推送时间: {}", message, created_time, sent_time)
                } else {
                    message.to_string()
                };
                
                let url = format!("https://api.telegram.org/bot{}/sendMessage", bot_token);
                let params = [("chat_id", chat_id.as_str()), ("text", final_message.as_str())];
                let response = client.post(&url).form(&params).send().await?;
                let status = response.status();
                if !status.is_success() {
                    let error_text = response.text().await.unwrap_or_else(|_| "未知错误".to_string());
                    anyhow::bail!("Telegram API 返回错误 (状态码: {}): {}", status, error_text);
                }
            }
            Notifier::Webhook {
                url,
                template,
                ignore_cache,
            } => {
                // 替换换行符为空格，避免 Webhook 不支持换行符
                let sanitized_message = message.replace('\n', " ");
                let key = webhook_template_key(url);
                let handlebar = TEMPLATE.read();
                let now = chrono::Local::now();
                let created_at_str = created_at
                    .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                    .unwrap_or_else(|| now.format("%Y-%m-%d %H:%M:%S").to_string());
                let sent_at_str = sent_at
                    .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                    .unwrap_or_else(|| now.format("%Y-%m-%d %H:%M:%S").to_string());
                let data = serde_json::json!({
                    "message": sanitized_message,
                    "created_at": created_at_str,
                    "sent_at": sent_at_str,
                });
                let payload = match ignore_cache {
                    Some(_) => handlebar.render_template(webhook_template_content(template), &data)?,
                    None => handlebar.render(&key, &data)?,
                };
                let response = client
                    .post(url)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(payload.clone())
                    .send()
                    .await?;
                let status = response.status();
                if !status.is_success() {
                    let error_text = response.text().await.unwrap_or_else(|_| "未知错误".to_string());
                    // 提供更详细的错误信息，包括发送的 payload
                    let error_msg = if status.as_u16() == 400 {
                        format!(
                            "Webhook 返回错误 (状态码: {}): {}\n\n实际发送的 Payload:\n{}\n\n提示：请检查模板格式是否符合目标 Webhook 的要求。",
                            status, error_text, payload
                        )
                    } else {
                        format!("Webhook 返回错误 (状态码: {}): {}", status, error_text)
                    };
                    anyhow::bail!("{}", error_msg);
                }
            }
        }
        Ok(())
    }
}
