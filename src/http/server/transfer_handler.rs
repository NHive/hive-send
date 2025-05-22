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
use crate::types::*;

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
    State(service): State<Arc<HttpTransferService>>,
    Path(request_id): Path<String>,
) -> Response {
    // 检查请求ID是否存在于待处理请求中
    if let Some(request) = service.pending_requests.get(&request_id) {
        // 根据请求状态构建响应
        let status_response = TransferStatusResponse {
            request_id: request_id.clone(),
            status: format!("{:?}", request.status).to_lowercase(),
            message: None,
        };

        (StatusCode::OK, Json(status_response)).into_response()
    }
    // 检查请求ID是否存在于已发送请求中
    else if let Some(request) = service.sent_requests.get(&request_id) {
        // 根据请求状态构建响应
        let status_response = TransferStatusResponse {
            request_id: request_id.clone(),
            status: format!("{:?}", request.status).to_lowercase(),
            message: None,
        };

        (StatusCode::OK, Json(status_response)).into_response()
    }
    // 请求ID不存在
    else {
        (
            StatusCode::NOT_FOUND,
            Json(ApiResponse::<()>::error(format!(
                "找不到传输请求: {}",
                request_id
            ))),
        )
            .into_response()
    }
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

/// 处理传输进度报告(发送方实现接口,接收方主动调用接口传输进度给发送方)
async fn handle_transfer_progress(
    State(service): State<Arc<HttpTransferService>>,
    Json(progress): Json<TransferProgress>,
) -> Response {
    println!("接收到传输进度报告: {:?}", progress);
    // 检查请求ID是否存在于已发送的请求列表中
    if !service.sent_requests.contains_key(&progress.request_id) {
        return (
            StatusCode::NOT_FOUND,
            Json(ApiResponse::<()>::error(format!(
                "找不到请求ID: {}",
                progress.request_id
            ))),
        )
            .into_response();
    }

    // 检查文件ID是否存在于待发送文件列表中
    let file_exists = service.sent_files.contains_key(&progress.file_id);
    if !file_exists {
        return (
            StatusCode::NOT_FOUND,
            Json(ApiResponse::<()>::error(format!(
                "找不到文件ID: {}",
                progress.file_id
            ))),
        )
            .into_response();
    }

    // 修改方式：先更新文件进度，然后释放锁，再发送事件
    {
        if let Some(mut file_info) = service.sent_files.get_mut(&progress.file_id) {
            // 只更新文件进度
            file_info.progress = progress.bytes_received;
        }
    } // 这里锁被释放

    // 根据状态进行相应处理
    match progress.status {
        TransferStatus::InProgress => {
            // 更新传输进度
            if let Err(e) = service.update_transfer_progress(progress.clone()).await {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiResponse::<()>::error(format!("更新传输进度失败: {}", e))),
                )
                    .into_response();
            }
        }
        TransferStatus::Completed => {
            // 先发送完成事件
            service.send_event(crate::types::TransferEvent::TransferCompleted {
                request_id: progress.request_id.clone(),
                file_id: progress.file_id.clone(),
                save_path: None,
            });

            // 然后再从发送列表移除文件
            service.sent_files.remove(&progress.file_id);
        }
        TransferStatus::Failed => {
            // 文件传输失败
            service.send_event(crate::types::TransferEvent::TransferFailed {
                request_id: progress.request_id.clone(),
                file_id: progress.file_id.clone(),
                error: progress
                    .error_message
                    .unwrap_or_else(|| "未知错误".to_string()),
            });
        }
        TransferStatus::Cancelled => {
            // 文件传输被取消
            service.send_event(crate::types::TransferEvent::TransferCancelled {
                request_id: progress.request_id.clone(),
                reason: progress
                    .error_message
                    .unwrap_or_else(|| "接收方取消".to_string()),
            });
        }
        _ => {
            // 无效的状态
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::<()>::error("无效的传输状态".to_string())),
            )
                .into_response();
        }
    }

    // 返回成功响应，使用204状态码表示已成功处理但无需返回内容
    StatusCode::NO_CONTENT.into_response()
}

/// 处理传输取消请求(发送方与接收方都需要实现,都可以通过该接口取消传输)
async fn handle_transfer_cancel(
    State(service): State<Arc<HttpTransferService>>,
    Json(cancel): Json<TransferCancel>,
) -> Response {
    // 检查请求ID是否存在
    let request_exists = service.pending_requests.contains_key(&cancel.request_id)
        || service.sent_requests.contains_key(&cancel.request_id);

    if !request_exists {
        return (
            StatusCode::NOT_FOUND,
            Json(ApiResponse::<()>::error(format!(
                "找不到请求ID: {}",
                cancel.request_id
            ))),
        )
            .into_response();
    }

    // 检查是否是接收到的请求
    if let Some(request) = service.pending_requests.get(&cancel.request_id) {
        // 如果是接收到的请求，清理相关文件记录
        for file in &request.files {
            // 移除任何与此请求相关的待接收文件
            service.received_files.remove(&file.file_id);
        }

        // 从待处理请求中移除
        service.pending_requests.remove(&cancel.request_id);
    }

    // 检查是否是发送的请求
    if let Some(request) = service.sent_requests.get(&cancel.request_id) {
        // 如果是发送的请求，清理相关文件记录
        for file in &request.files {
            // 移除任何与此请求相关的待发送文件
            service.sent_files.remove(&file.file_id);
        }

        // 从已发送请求中移除
        service.sent_requests.remove(&cancel.request_id);
    }

    // 发送取消事件
    service.send_event(crate::types::TransferEvent::TransferCancelled {
        request_id: cancel.request_id.clone(),
        reason: cancel.reason.clone(),
    });

    // 返回成功，无需内容
    StatusCode::NO_CONTENT.into_response()
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
        // 取消传输
        .route("/api/transfer/cancel", post(handle_transfer_cancel))
}
