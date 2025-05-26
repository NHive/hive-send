use std::net::SocketAddr;
use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{ConnectInfo, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};

use crate::dto::response::DeviceStatusInfo;
use crate::http::HttpTransferService;
use crate::types::DeviceInfo;

/// 获取设备状态 (GET请求)
async fn get_device_status(State(service): State<Arc<HttpTransferService>>) -> impl IntoResponse {
    let status = service.get_device_status().await;
    (StatusCode::OK, Json(status))
}

/// 获取设备状态 (POST请求)，并处理对方设备信息
async fn check_device_status(
    State(service): State<Arc<HttpTransferService>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(remote_status): Json<DeviceStatusInfo>,
) -> impl IntoResponse {
    // 获取发送方IP地址
    let sender_ip = addr.ip().to_string();

    // 检查设备是否在已知设备列表中
    let device_exists = service.known_devices.contains_key(&remote_status.device_id);

    // 如果设备不存在，添加到已知设备列表
    if !device_exists {
        let device_info = DeviceInfo {
            device_id: remote_status.device_id.clone(),
            address: vec![sender_ip],
            port: remote_status.port,
            is_online: remote_status.is_online,
            version: remote_status.version.clone(),
        };

        // 添加设备并发送设备发现事件
        service
            .known_devices
            .insert(remote_status.device_id.clone(), device_info.clone());
        service.send_event(crate::types::TransferEvent::DeviceDiscovered(device_info));
    }

    // 返回本地设备状态
    let status = service.get_device_status().await;
    (StatusCode::OK, Json(status))
}

/// 注册状态相关路由
pub fn register() -> Router<Arc<HttpTransferService>> {
    Router::new()
        .route("/api/status", get(get_device_status))
        .route("/api/status", post(check_device_status))
}
