// file_path: src/services/transfer/http/server/transfer_handler.rs
use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use std::sync::Arc;

use crate::dto::request::*;
use crate::dto::response::*;
use crate::http::ApiResponse;
use crate::http::HttpTransferService;

/// 处理文件传输请求
async fn handle_transfer_request(
    State(service): State<Arc<HttpTransferService>>,
    Json(request): Json<TransferRequest>,
) -> Response {
    // 将请求添加到待处理列表
    match service.add_pending_request(request.clone()).await {
        Ok(_) => {
            // 成功添加请求，返回请求ID和pending状态
            let response = TransferRequestResponse {
                request_id: request.request_id.clone(),
                status: "pending".to_string(),
            };

            (StatusCode::OK, Json(response)).into_response()
        }
        Err(e) => {
            // 请求处理失败
            (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::<()>::error(format!("处理传输请求失败: {}", e))),
            )
                .into_response()
        }
    }
}

/// 查询传输请求状态
async fn get_transfer_status(
    State(_): State<Arc<HttpTransferService>>,
    Path(request_id): Path<String>,
) -> Response {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(ApiResponse::<()>::error(format!(
            "获取传输状态功能尚未实现，请求ID: {}",
            request_id
        ))),
    )
        .into_response()
}

/// 处理传输响应,接收方接受或拒绝后会调用该接口通知发送方
async fn handle_transfer_response(
    State(service): State<Arc<HttpTransferService>>,
    Json(response): Json<TransferResponse>,
) -> Response {
    // 检查请求ID是否存在于发送的请求列表中
    if let Some(request) = service.sent_requests.get(&response.request_id) {
        // 检查响应状态
        match response.status.as_str() {
            "accepted" => {
                // 发送事件通知状态更新
                service.send_event(crate::types::TransferEvent::RequestStatusChanged {
                    request_id: response.request_id.clone(),
                    status: "accepted".to_string(),
                    message: None,
                });

                // 返回成功响应
                (
                    StatusCode::OK,
                    Json(ApiResponse::<()>::error("传输请求已接受".to_string())),
                )
                    .into_response()
            }
            "rejected" => {
                // 接收方拒绝了请求，删除待发送的文件记录
                for file in &request.files {
                    service.sent_files.remove(&file.file_id);
                }

                // 发送事件通知状态更新
                service.send_event(crate::types::TransferEvent::RequestStatusChanged {
                    request_id: response.request_id.clone(),
                    status: "rejected".to_string(),
                    message: None,
                });

                // 返回拒绝响应
                (
                    StatusCode::OK,
                    Json(ApiResponse::<()>::error("传输请求已被拒绝".to_string())),
                )
                    .into_response()
            }
            _ => {
                // 状态值无效
                (
                    StatusCode::BAD_REQUEST,
                    Json(ApiResponse::<()>::error("无效的响应状态".to_string())),
                )
                    .into_response()
            }
        }
    } else {
        // 请求ID不存在
        (
            StatusCode::NOT_FOUND,
            Json(ApiResponse::<()>::error(format!(
                "找不到请求ID: {}",
                response.request_id
            ))),
        )
            .into_response()
    }
}

/// 处理传输进度报告
#[axum::debug_handler]
async fn handle_transfer_progress(
    State(_): State<Arc<HttpTransferService>>,
    Json(progress): Json<TransferProgress>,
) -> Response {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(ApiResponse::<()>::error(
            "传输进度处理功能尚未实现".to_string(),
        )),
    )
        .into_response()
}

/// 处理传输取消请求
#[axum::debug_handler]
async fn handle_transfer_cancel(
    State(_): State<Arc<HttpTransferService>>,
    Json(cancel): Json<TransferCancel>,
) -> Response {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(ApiResponse::<()>::error("传输取消功能尚未实现".to_string())),
    )
        .into_response()
}

/// 处理文件验证请求
#[axum::debug_handler]
async fn handle_transfer_verify(
    State(_): State<Arc<HttpTransferService>>,
    Json(verify): Json<TransferVerify>,
) -> Response {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(ApiResponse::<()>::error("文件验证功能尚未实现".to_string())),
    )
        .into_response()
}

/// 注册传输相关路由
pub fn register() -> Router<Arc<HttpTransferService>> {
    Router::new()
        // 请求传输
        .route("/api/transfer/request", post(handle_transfer_request))
        // 查询传输状态
        .route(
            "/api/transfer/request/{request_id}/status",
            get(get_transfer_status),
        )
        // 响应传输请求
        .route("/api/transfer/response", post(handle_transfer_response))
        // 接收下载方的进度报告
        .route("/api/transfer/progress", post(handle_transfer_progress))
        .route("/api/transfer/cancel", post(handle_transfer_cancel))
        .route("/api/transfer/verify", post(handle_transfer_verify))
}
