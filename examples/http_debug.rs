use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::{get, post},
};

use hive_send::{
    TransferService, create_transfer_service,
    dto::{
        request::TransferRequest,
        response::{ApiResponse, DeviceStatusInfo, DiscoveryLevel, TransferRequestResponse},
    },
    types::{
        PendingReceiveFileInfo, PendingSendFileInfo, TransferEvent, TransferImplementation,
        TransferServiceConfig,
    },
};
use serde::{Deserialize, Serialize};

#[derive(Clone)]
struct AppState {
    transfer_service: Arc<dyn TransferService>,
}

// 查询参数结构体
#[derive(Debug, Deserialize)]
struct ScanDeviceQuery {
    address: String,
    port: Option<u16>,
}

#[derive(Debug, Deserialize)]
struct CreateRequestQuery {
    file_path: String,
    device_id: String,
}

#[derive(Debug, Serialize)]
struct ServiceStatusResponse {
    running: bool,
    device_id: String,
}

// 启动HTTP调试服务器
async fn start_debug_server() -> Result<(), Box<dyn std::error::Error>> {
    // 设置日志
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    println!("创建传输服务...");

    // 创建传输服务配置
    let config = TransferServiceConfig {
        server_port: 6712,
        discovery_level: DiscoveryLevel::AllNearby,
        ..Default::default()
    };

    // 创建传输服务实例
    let transfer_service = create_transfer_service(TransferImplementation::Http, config).await?;

    // 启动传输服务
    transfer_service.start().await?;

    // 创建应用状态
    let app_state = AppState {
        transfer_service: transfer_service.clone(),
    };

    // 创建事件监听器
    let event_rx = transfer_service.subscribe();
    tokio::spawn(async move {
        listen_for_events(event_rx).await;
    });

    // 构建路由
    let app = Router::new()
        .route("/api/status", get(get_status))
        .route("/api/service/status", get(get_service_status))
        .route("/api/service/start", post(start_service))
        .route("/api/service/stop", post(stop_service))
        .route("/api/devices", get(get_devices))
        .route("/api/devices/scan", get(scan_device))
        .route("/api/transfers/requests", get(get_transfer_requests))
        .route("/api/transfers/sent_requests", get(get_sent_requests))
        .route("/api/transfers/create", post(create_transfer_request))
        .route("/api/transfers/{request_id}/accept", post(accept_transfer))
        .route("/api/transfers/{request_id}/reject", post(reject_transfer))
        .route("/api/transfers/send_files", get(get_pending_send_files)) // 新添加的路由
        .route(
            "/api/transfers/receive_files",
            get(get_pending_receive_files),
        ) // 新添加的路由
        .with_state(app_state);

    // 定义debug服务器的监听地址
    let listener = tokio::net::TcpListener::bind("0.0.0.0:6711").await.unwrap();

    axum::serve(listener, app).await.unwrap();

    Ok(())
}

// 监听传输事件并打印
async fn listen_for_events(mut rx: tokio::sync::broadcast::Receiver<TransferEvent>) {
    while let Ok(event) = rx.recv().await {
        println!("收到事件: {:?}", event);
    }
}

// 获取服务状态
async fn get_status(State(state): State<AppState>) -> Json<ApiResponse<DeviceStatusInfo>> {
    let service = &state.transfer_service;
    let device_status = match service
        .as_any()
        .downcast_ref::<hive_send::http::HttpTransferService>()
    {
        Some(http_service) => http_service.get_device_status().await,
        None => {
            return Json(ApiResponse::error("无法获取设备状态".into()));
        }
    };

    Json(ApiResponse {
        success: true,
        data: Some(device_status),
        message: None,
    })
}

// 获取服务运行状态
async fn get_service_status(
    State(state): State<AppState>,
) -> Json<ApiResponse<ServiceStatusResponse>> {
    let service = &state.transfer_service;
    let is_running = match service
        .as_any()
        .downcast_ref::<hive_send::http::HttpTransferService>()
    {
        Some(http_service) => http_service.is_service_running().await,
        None => false,
    };

    let response = ServiceStatusResponse {
        running: is_running,
        device_id: "device_id_placeholder".to_string(), // 这里可以替换为实际的设备ID
    };

    Json(ApiResponse {
        success: true,
        data: Some(response),
        message: None,
    })
}

// 启动服务
async fn start_service(State(state): State<AppState>) -> Json<ApiResponse<()>> {
    let service = &state.transfer_service;
    match service.start().await {
        Ok(_) => Json(ApiResponse {
            success: true,
            data: None,
            message: Some("服务已启动".to_string()),
        }),
        Err(e) => Json(ApiResponse::error(format!("启动服务失败: {}", e))),
    }
}

// 停止服务
async fn stop_service(State(state): State<AppState>) -> Json<ApiResponse<()>> {
    let service = &state.transfer_service;
    match service.stop().await {
        Ok(_) => Json(ApiResponse {
            success: true,
            data: None,
            message: Some("服务已停止".to_string()),
        }),
        Err(e) => Json(ApiResponse::error(format!("停止服务失败: {}", e))),
    }
}

// 获取已知设备列表
async fn get_devices(
    State(state): State<AppState>,
) -> Json<ApiResponse<Vec<hive_send::types::DeviceInfo>>> {
    let service = &state.transfer_service;
    match service.get_known_devices().await {
        Ok(devices) => Json(ApiResponse {
            success: true,
            data: Some(devices),
            message: None,
        }),
        Err(e) => Json(ApiResponse::error(format!("获取设备列表失败: {}", e))),
    }
}

// 扫描设备
async fn scan_device(
    State(state): State<AppState>,
    Query(params): Query<ScanDeviceQuery>,
) -> Json<ApiResponse<hive_send::types::DeviceInfo>> {
    let service = &state.transfer_service;
    let port = params.port.unwrap_or(61314); // 默认端口

    match service.scan_device(&params.address, port).await {
        Ok(device) => Json(ApiResponse {
            success: true,
            data: Some(device),
            message: None,
        }),
        Err(e) => Json(ApiResponse::error(format!("扫描设备失败: {}", e))),
    }
}

// 获取传输请求列表, 接收到的请求
async fn get_transfer_requests(
    State(state): State<AppState>,
) -> Json<ApiResponse<Vec<TransferRequest>>> {
    let service = &state.transfer_service;

    // 获取已接收的请求
    match service.get_received_requests().await {
        Ok(requests) => Json(ApiResponse {
            success: true,
            data: Some(requests),
            message: None,
        }),
        Err(e) => Json(ApiResponse::error(format!("获取传输请求失败: {}", e))),
    }
}

// 获取已发送的传输请求
async fn get_sent_requests(
    State(state): State<AppState>,
) -> Json<ApiResponse<Vec<TransferRequest>>> {
    let service = &state.transfer_service;

    // 获取已发送的请求
    match service.get_sent_requests().await {
        Ok(requests) => Json(ApiResponse {
            success: true,
            data: Some(requests),
            message: None,
        }),
        Err(e) => Json(ApiResponse::error(format!("获取传输请求失败: {}", e))),
    }
}

// 创建传输请求
async fn create_transfer_request(
    State(state): State<AppState>,
    Query(params): Query<CreateRequestQuery>,
) -> Json<ApiResponse<TransferRequestResponse>> {
    let service = &state.transfer_service;

    match service
        .create_transfer_request_by_path(&params.file_path, &params.device_id)
        .await
    {
        Ok(response) => Json(ApiResponse {
            success: true,
            data: Some(response),
            message: None,
        }),
        Err(e) => Json(ApiResponse::error(format!("创建传输请求失败: {}", e))),
    }
}

// 接受传输请求
async fn accept_transfer(
    State(state): State<AppState>,
    Path(request_id): Path<String>,
) -> Json<ApiResponse<()>> {
    let service = &state.transfer_service;

    match service
        .accept_transfer_request(&request_id, "/Users/chenzibo/Downloads", Vec::new())
        .await
    {
        Ok(_) => Json(ApiResponse {
            success: true,
            data: None,
            message: Some("已接受传输请求".to_string()),
        }),
        Err(e) => Json(ApiResponse::error(format!("接受传输请求失败: {}", e))),
    }
}

// 拒绝传输请求
async fn reject_transfer(
    State(state): State<AppState>,
    Path(request_id): Path<String>,
) -> Json<ApiResponse<()>> {
    let service = &state.transfer_service;

    match service.reject_transfer_request(&request_id, None).await {
        Ok(_) => Json(ApiResponse {
            success: true,
            data: None,
            message: Some("已拒绝传输请求".to_string()),
        }),
        Err(e) => Json(ApiResponse::error(format!("拒绝传输请求失败: {}", e))),
    }
}

// 获取待发送文件列表的处理函数
async fn get_pending_send_files(
    State(state): State<AppState>,
) -> Json<ApiResponse<Vec<PendingSendFileInfo>>> {
    let service = &state.transfer_service;

    match service.get_pending_send_files().await {
        Ok(files) => Json(ApiResponse {
            success: true,
            data: Some(files),
            message: None,
        }),
        Err(e) => Json(ApiResponse::error(format!("获取待发送文件列表失败: {}", e))),
    }
}

// 获取待接收文件列表的处理函数
async fn get_pending_receive_files(
    State(state): State<AppState>,
) -> Json<ApiResponse<Vec<PendingReceiveFileInfo>>> {
    let service = &state.transfer_service;

    match service.get_pending_receive_files().await {
        Ok(files) => Json(ApiResponse {
            success: true,
            data: Some(files),
            message: None,
        }),
        Err(e) => Json(ApiResponse::error(format!("获取待接收文件列表失败: {}", e))),
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("启动HTTP调试服务器...");
    start_debug_server().await?;
    Ok(())
}
