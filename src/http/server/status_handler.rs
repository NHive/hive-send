use std::sync::Arc;

use axum::{Json, Router, extract::State, http::StatusCode, response::IntoResponse, routing::get};

use crate::http::HttpTransferService;

/// 获取设备状态
async fn get_device_status(State(service): State<Arc<HttpTransferService>>) -> impl IntoResponse {
    let status = service.get_device_status().await;
    (StatusCode::OK, Json(status))
}

/// 注册状态相关路由
pub fn register() -> Router<Arc<HttpTransferService>> {
    Router::new().route("/api/status", get(get_device_status))
}
