mod queue;
mod global;

use anyhow::Result;
use futures::future;
use reqwest::header;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::config::TEMPLATE;

pub use queue::NotificationQueue;
pub use global::NOTIFICATION_QUEUE;

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

pub fn webhook_template_key(url: &str) -> String {
    format!("payload_{}", url)
}

pub fn webhook_template_content(template: &Option<String>) -> &str {
    template
        .as_deref()
        .filter(|t| !t.trim().is_empty())
        .unwrap_or(r#"{"text": "{{{message}}}"}"#)
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
    pub async fn notify(&self, client: &reqwest::Client, message: &str) -> Result<()> {
        match self {
            Notifier::Telegram { bot_token, chat_id } => {
                let url = format!("https://api.telegram.org/bot{}/sendMessage", bot_token);
                let params = [("chat_id", chat_id.as_str()), ("text", message)];
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
                let data = serde_json::json!({
                    "message": sanitized_message,
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
