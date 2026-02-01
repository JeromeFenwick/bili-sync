use std::sync::Arc;

use anyhow::Result;
use axum::extract::Extension;
use axum::routing::{get, post};
use axum::{Json, Router};
use sea_orm::DatabaseConnection;

use serde::Serialize;

use crate::api::wrapper::{ApiError, ApiResponse, ValidatedJson};
use crate::bilibili::BiliClient;
use crate::config::{Config, VersionedConfig};
use crate::notifier::Notifier;

#[derive(Serialize)]
pub struct TestNotifierResponse {
    pub success: bool,
    pub message: String,
    pub details: Option<String>,
}

pub(super) fn router() -> Router {
    Router::new()
        .route("/config", get(get_config).put(update_config))
        .route("/config/notifiers/ping", post(ping_notifiers))
}

/// 获取全局配置
pub async fn get_config() -> Result<ApiResponse<Arc<Config>>, ApiError> {
    Ok(ApiResponse::ok(VersionedConfig::get().snapshot()))
}

/// 更新全局配置
pub async fn update_config(
    Extension(db): Extension<DatabaseConnection>,
    ValidatedJson(config): ValidatedJson<Config>,
) -> Result<ApiResponse<Arc<Config>>, ApiError> {
    config.check()?;
    let new_config = VersionedConfig::get().update(config, &db).await?;
    Ok(ApiResponse::ok(new_config))
}

pub async fn ping_notifiers(
    Extension(bili_client): Extension<Arc<BiliClient>>,
    Json(mut notifier): Json<Notifier>,
) -> Result<ApiResponse<TestNotifierResponse>, ApiError> {
    let test_message = "✅ 测试通知\n\n这是一条来自 BiliSync 的测试通知，如果您收到此消息，说明通知配置正常。";
    
    // 对于 webhook 类型的通知器测试，设置上 ignore_cache tag 以强制实时渲染
    if let Notifier::Webhook { ignore_cache, .. } = &mut notifier {
        *ignore_cache = Some(());
    }
    
    // 尝试发送通知并捕获详细错误
    match notifier.notify(bili_client.inner_client(), test_message).await {
        Ok(_) => {
            Ok(ApiResponse::ok(TestNotifierResponse {
                success: true,
                message: "测试通知已发送".to_string(),
                details: match &notifier {
                    Notifier::Telegram { .. } => Some("请检查 Telegram 是否收到消息".to_string()),
                    Notifier::Webhook { url, .. } => Some(format!("已发送到: {}", url)),
                },
            }))
        }
        Err(e) => {
            let error_msg = format!("{:#}", e);
            let details = match &notifier {
                Notifier::Telegram { .. } => {
                    Some("请检查 Bot Token 和 Chat ID 是否正确，以及网络连接是否正常".to_string())
                }
                Notifier::Webhook { url, .. } => {
                    Some(format!("请检查 Webhook URL ({}) 是否可访问，以及模板格式是否正确", url))
                }
            };
            
            Ok(ApiResponse::ok(TestNotifierResponse {
                success: false,
                message: format!("测试通知发送失败: {}", error_msg),
                details,
            }))
        }
    }
}
