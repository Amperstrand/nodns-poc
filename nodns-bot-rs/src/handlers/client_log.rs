use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Json;
use serde::Deserialize;
use serde_json::Value;
use tracing::warn;

use crate::AppState;

#[derive(Deserialize)]
pub struct ClientLogRequest {
    pub errors: Vec<Value>,
}

pub async fn client_log_handler(
    State(_state): State<std::sync::Arc<AppState>>,
    Json(payload): Json<ClientLogRequest>,
) -> StatusCode {
    for entry in &payload.errors {
        let error_type = entry
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let message = entry.get("message").and_then(|v| v.as_str()).unwrap_or("");
        let url = entry.get("url").and_then(|v| v.as_str()).unwrap_or("");
        let user_agent = entry
            .get("userAgent")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        warn!(
            error_type = %error_type,
            message = %message,
            url = %url,
            user_agent = %user_agent,
            "client-side error report"
        );
    }

    StatusCode::OK
}
