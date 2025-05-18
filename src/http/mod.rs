pub mod client;
pub mod download_manage;
pub mod server;

use async_trait::async_trait;
use dashmap::DashMap;
use log::{debug, error, info, warn};
use std::any::Any;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

use crate::TransferService;
use crate::error::{HiveDropError, Result};

use crate::dto::request::*;
use crate::dto::response::*;
use crate::types::*;

use crate::utils::file::path_to_transfer_structure;

/// HTTP 传输服务实现
#[derive(Clone)]
pub struct HttpTransferService {
    /// 服务配置
    config: TransferServiceConfig,
    /// 事件发送器
    event_sender: broadcast::Sender<TransferEvent>,
    /// 服务状态
    is_running: Arc<Mutex<bool>>,
    /// 已知设备缓存 (device_id -> DeviceInfo)
    known_devices: Arc<DashMap<String, DeviceInfo>>,
    /// 服务器实例管理器
    server_handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    /// 下载管理器句柄
    download_handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    /// 已接收到的传输请求缓存(request_id -> TransferRequest),待处理或者未处理完毕的请求
    pending_requests: Arc<DashMap<String, TransferRequest>>,
    /// 已发送成功的传输请求缓存(request_id -> TransferRequest),待对方确认或者未处理完毕的请求
    sent_requests: Arc<DashMap<String, TransferRequest>>,
    /// 待发送文件列表(file_id -> PendingSendFileInfo),待发送的文件信息
    sent_files: Arc<DashMap<String, PendingSendFileInfo>>,
    /// 待接收文件列表(file_id -> PendingReceiveFileInfo),待接收的文件信息
    received_files: Arc<DashMap<String, PendingReceiveFileInfo>>,
}

impl HttpTransferService {
    /// 创建新的 HTTP 传输服务
    pub async fn new(config: TransferServiceConfig) -> Self {
        let (sender, _) = broadcast::channel(100); // 创建容量为100的广播通道

        Self {
            config,
            event_sender: sender,
            is_running: Arc::new(Mutex::new(false)),
            known_devices: Arc::new(DashMap::new()),
            server_handle: Arc::new(Mutex::new(None)),
            download_handle: Arc::new(Mutex::new(None)),
            pending_requests: Arc::new(DashMap::new()),
            sent_requests: Arc::new(DashMap::new()),
            sent_files: Arc::new(DashMap::new()),
            received_files: Arc::new(DashMap::new()),
        }
    }

    /// 创建一个新的 HTTP 传输服务实例并包装在 Arc 中
    pub async fn create(config: TransferServiceConfig) -> Arc<Self> {
        Arc::new(Self::new(config).await)
    }

    /// 发送服务事件
    fn send_event(&self, event: TransferEvent) {
        if let Err(e) = self.event_sender.send(event) {
            warn!("发送事件失败: {}", e);
        }
    }

    /// 获取服务配置
    pub fn get_config(&self) -> &TransferServiceConfig {
        &self.config
    }

    /// 获取服务器端口
    pub fn get_server_port(&self) -> u16 {
        self.config.server_port
    }

    /// 获取设备的状态信息
    pub async fn get_device_status(&self) -> DeviceStatusInfo {
        DeviceStatusInfo {
            device_id: self.config.device_id.clone(),
            device_name: self.config.device_name.clone(),
            user_id: self.config.user_id.clone(),
            discovery_level: self.config.discovery_level.clone(),
            is_online: self.is_service_running().await,
            version: env!("CARGO_PKG_VERSION").to_string(),
            port: self.config.server_port,
        }
    }

    /// 检查服务是否正在运行
    pub async fn is_service_running(&self) -> bool {
        *self.is_running.lock().unwrap()
    }

    /// 添加新接收到的传输请求到缓存
    pub async fn add_pending_request(&self, request: TransferRequest) -> Result<()> {
        // 检查是否已存在相同ID的请求
        if self.pending_requests.contains_key(&request.request_id) {
            return Err(HiveDropError::ValidationError(
                "传输请求ID已存在".to_string(),
            ));
        }

        // 添加请求并发送事件
        self.pending_requests
            .insert(request.request_id.clone(), request.clone());
        self.send_event(TransferEvent::RequestReceived(request));

        Ok(())
    }

    /// 添加已发送的传输请求到缓存
    pub async fn add_sent_request(&self, request: TransferRequest) -> Result<()> {
        // 检查是否已存在相同ID的请求
        if self.sent_requests.contains_key(&request.request_id) {
            return Err(HiveDropError::ValidationError(
                "传输请求ID已存在".to_string(),
            ));
        }

        // 添加请求到缓存
        self.sent_requests
            .insert(request.request_id.clone(), request.clone());

        Ok(())
    }

    /// 更新传输请求状态
    pub async fn update_request_status(
        &self,
        request_id: &str,
        status: TransferStatus,
        message: Option<String>,
    ) -> Result<()> {
        if let Some(_request_entry) = self.pending_requests.get_mut(request_id) {
            // 发送状态变更事件
            self.send_event(TransferEvent::RequestStatusChanged {
                request_id: request_id.to_string(),
                status: format!("{:?}", status),
                message: message.clone(),
            });

            Ok(())
        } else {
            Err(HiveDropError::NotFoundError(format!(
                "找不到传输请求: {}",
                request_id
            )))
        }
    }

    /// 获取设备信息（如果没有则返回None）
    pub async fn get_device_info(&self, device_id: &str) -> Option<DeviceInfo> {
        self.known_devices.get(device_id).map(|dev| dev.clone())
    }

    /// 更新传输进度
    pub async fn update_transfer_progress(&self, progress: TransferProgress) -> Result<()> {
        // 发送进度更新事件
        self.send_event(TransferEvent::ProgressUpdated(progress));

        Ok(())
    }

    /// 添加需要接收的文件到待接收列表
    pub async fn add_pending_receive_file(&self, file_info: PendingReceiveFileInfo) -> Result<()> {
        // 检查是否已存在相同ID的文件
        if self.received_files.contains_key(&file_info.file_id) {
            return Err(HiveDropError::ValidationError(
                "接收文件ID已存在".to_string(),
            ));
        }

        // 添加文件到待接收列表
        self.received_files
            .insert(file_info.file_id.clone(), file_info);

        Ok(())
    }
}

#[async_trait]
impl TransferService for HttpTransferService {
    async fn create_transfer_request_by_path(
        &self,
        file_path: &str,
        device_id: &str,
    ) -> Result<TransferRequestResponse> {
        // 单路径请求直接调用多路径方法
        self.create_transfer_request_by_paths(vec![file_path.to_string()], device_id, None)
            .await
    }

    async fn create_transfer_request_by_paths(
        &self,
        file_paths: Vec<String>,
        device_id: &str,
        excluded_patterns: Option<Vec<String>>,
    ) -> Result<TransferRequestResponse> {
        let request_id = uuid::Uuid::new_v4().to_string();
        let mut all_files = Vec::new();

        // 处理所有提供的文件路径
        for file_path in file_paths {
            debug!("处理路径: {}", file_path);
            let path_files = path_to_transfer_structure(&file_path, excluded_patterns.clone())?;

            // 添加该路径的所有文件到总列表
            for file in path_files {
                self.sent_files.insert(file.file_id.clone(), file.clone());
                all_files.push(file);
            }
        }

        debug!("共处理 {} 个文件/目录", all_files.len());

        if all_files.is_empty() {
            return Err(HiveDropError::ValidationError(
                "没有找到有效文件可发送".to_string(),
            ));
        }

        let request = TransferRequest {
            request_id: request_id.clone(),
            sender_device_id: self.config.device_id.clone(),
            sender_device_name: self.config.device_name.clone(),
            receiver_device_id: device_id.to_string(),
            files: all_files.iter().map(|file| file.to_file_info()).collect(),
        };

        // 添加请求到已发送请求列表
        self.add_sent_request(request.clone()).await?;

        // 获取接收方的地址和端口
        let receiver_info = self
            .get_device_info(device_id)
            .await
            .ok_or_else(|| HiveDropError::NotFoundError("接收方设备未找到".to_string()))?;

        let client = client::HttpClient::new()?;

        let response = client
            .send_transfer_request(&request, &receiver_info.address[0], receiver_info.port)
            .await
            .map_err(|e| HiveDropError::NetworkError(format!("发送请求失败: {}", e)))?;

        Ok(response)
    }

    async fn accept_transfer_request(
        &self,
        request_id: &str,
        save_path: &str,
        exclusion_list: Vec<String>,
    ) -> Result<()> {
        debug!("接受传输请求: {}", request_id);

        // 更新请求状态
        self.update_request_status(request_id, TransferStatus::Accepted, None)
            .await?;

        let send_device_id = self
            .pending_requests
            .get(request_id)
            .ok_or_else(|| HiveDropError::NotFoundError("请求ID不存在".to_string()))?
            .sender_device_id
            .clone();

        // 添加需要接收的文件到待接收列表
        if let Some(request) = self.pending_requests.get(request_id) {
            for file in &request.files {
                // 检查排除列表
                if exclusion_list.contains(&file.file_id) {
                    continue;
                }

                // 检查文件路径是否存在
                let pending_file_info = PendingReceiveFileInfo {
                    device_id: send_device_id.clone(),
                    file_id: file.file_id.clone(),
                    file_name: file.file_name.clone(),
                    file_relative_path: file.file_path.clone(),
                    // 保存路径+相对路径
                    file_absolute_path: format!("{}/{}", save_path, file.file_path),
                    file_size: file.file_size,
                    mime_type: file.mime_type.clone(),
                    is_dir: file.is_dir,
                    progress: 0,
                    status: TransferStatus::Accepted,
                };
                self.received_files
                    .insert(file.file_id.clone(), pending_file_info);
            }
        }

        Ok(())
    }

    async fn reject_transfer_request(
        &self,
        request_id: &str,
        reason: Option<String>,
    ) -> Result<()> {
        debug!("拒绝传输请求: {}, 原因: {:?}", request_id, reason);

        // 更新请求状态
        self.update_request_status(request_id, TransferStatus::Rejected, reason)
            .await?;

        Ok(())
    }

    /// 取消传输请求(发送方)
    async fn cancel_transfer_request(&self, request_id: &str) -> Result<()> {
        debug!("取消传输请求: {}", request_id);

        Ok(())
    }

    /// 获取所有待处理的传输请求
    async fn get_received_requests(&self) -> Result<Vec<TransferRequest>> {
        let pending_list = self
            .pending_requests
            .iter()
            .map(|entry| entry.value().clone())
            .collect();

        Ok(pending_list)
    }

    /// 获取所有已发送的传输请求(活动请求),定时清理已完成的请求,或未收到响应的请求
    async fn get_sent_requests(&self) -> Result<Vec<TransferRequest>> {
        let sent_list = self
            .sent_requests
            .iter()
            .map(|entry| entry.value().clone())
            .collect();

        Ok(sent_list)
    }

    async fn pause_file_download(&self, request_id: &str, file_id: &str) -> Result<()> {
        debug!("暂停文件下载: 请求ID={}, 文件ID={}", request_id, file_id);

        // 暂停下载的逻辑

        Ok(())
    }

    async fn resume_file_download(&self, request_id: &str, file_id: &str) -> Result<()> {
        debug!("恢复文件下载: 请求ID={}, 文件ID={}", request_id, file_id);

        // 恢复下载的逻辑

        Ok(())
    }

    async fn cancel_file_receiving(&self, request_id: &str, reason: &str) -> Result<()> {
        debug!("取消传输: 请求ID={}, 原因={}", request_id, reason);

        // 取消传输的逻辑
        self.send_event(TransferEvent::TransferCancelled {
            request_id: request_id.to_string(),
            reason: reason.to_string(),
        });

        Ok(())
    }

    async fn cancel_file_send(&self, request_id: &str, file_id: &str) -> Result<()> {
        debug!("取消文件发送: 请求ID={}, 文件ID={}", request_id, file_id);

        // 取消文件发送的逻辑

        Ok(())
    }

    async fn verify_file(&self, request_id: &str, file_id: &str) -> Result<VerifyResponse> {
        debug!("验证文件: 请求ID={}, 文件ID={}", request_id, file_id);

        // 验证文件的逻辑

        Ok(VerifyResponse { is_valid: true })
    }

    fn get_user_id(&self) -> String {
        self.config.user_id.clone()
    }

    async fn set_user_id(&self, user_id: &str) -> Result<()> {
        let old_user_id = self.config.user_id.clone();

        // 这里需要修改字段，使用unsafe绕过不可变性约束
        unsafe {
            let config_ptr =
                &self.config as *const TransferServiceConfig as *mut TransferServiceConfig;
            (*config_ptr).user_id = user_id.to_string();
        }

        // 发送事件
        self.send_event(TransferEvent::UserIdChanged {
            device_id: self.config.device_id.clone(),
            old_user_id,
            new_user_id: user_id.to_string(),
        });

        Ok(())
    }

    fn get_discovery_level(&self) -> DiscoveryLevel {
        self.config.discovery_level.clone()
    }

    async fn set_discovery_level(&self, level: DiscoveryLevel) -> Result<()> {
        // 使用unsafe修改字段，绕过不可变性约束
        unsafe {
            let config_ptr =
                &self.config as *const TransferServiceConfig as *mut TransferServiceConfig;
            (*config_ptr).discovery_level = level.clone();
        }

        // 发送事件
        self.send_event(TransferEvent::DiscoveryLevelChanged {
            device_id: self.config.device_id.clone(),
            level,
        });

        Ok(())
    }

    fn subscribe(&self) -> broadcast::Receiver<TransferEvent> {
        self.event_sender.subscribe()
    }

    async fn start(&self) -> Result<()> {
        // 标记服务为运行状态
        {
            let mut is_running = self.is_running.lock().unwrap();
            if *is_running {
                debug!("HTTP 传输服务已在运行中");
                return Ok(());
            }
            *is_running = true;
        }

        info!("正在启动 HTTP 传输服务...");
        // 启动 HTTP 服务器，传递服务实例的Arc
        let service_for_server: Arc<HttpTransferService> = Arc::new(self.clone());
        {
            let mut server_handle = self.server_handle.lock().unwrap();
            *server_handle = Some(tokio::spawn(async move {
                if let Err(e) = server::start(service_for_server).await {
                    error!("HTTP 服务器启动失败: {:?}", e);
                }
            }));
        }

        // 启动下载管理器
        let service_for_download: Arc<HttpTransferService> = Arc::new(self.clone());
        {
            let mut download_handle = self.download_handle.lock().unwrap();
            *download_handle = Some(tokio::spawn(async move {
                if let Err(e) = download_manage::start(service_for_download).await {
                    error!("下载管理器启动失败: {:?}", e);
                }
            }));
        }

        self.send_event(TransferEvent::ServiceStarted);
        info!("HTTP 传输服务已启动");

        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        let mut is_running = self.is_running.lock().unwrap();
        if !*is_running {
            debug!("HTTP 传输服务未运行");
            return Ok(());
        }
        *is_running = false;

        info!("正在停止 HTTP 传输服务...");

        // 停止 HTTP 服务器
        let mut server_handle = self.server_handle.lock().unwrap();
        if let Some(handle) = server_handle.take() {
            handle.abort();
            debug!("HTTP 服务器任务已中止");
        }

        // 停止下载管理器
        let mut download_handle = self.download_handle.lock().unwrap();
        if let Some(handle) = download_handle.take() {
            handle.abort();
            debug!("下载管理器任务已中止");
        }

        self.send_event(TransferEvent::ServiceStopped);
        info!("HTTP 传输服务已停止");

        Ok(())
    }

    async fn shutdown(&self) -> Result<()> {
        self.stop().await?;
        // 清理资源
        self.known_devices.clear();
        self.pending_requests.clear();
        self.sent_requests.clear();

        Ok(())
    }

    async fn retry_failed_transfer(&self, request_id: &str, file_id: &str) -> Result<()> {
        debug!("重试失败的传输: 请求ID={}, 文件ID={}", request_id, file_id);

        // 重试传输的逻辑

        Ok(())
    }

    async fn get_known_devices(&self) -> Result<Vec<DeviceInfo>> {
        // 从缓存返回已知设备列表
        let device_list: Vec<DeviceInfo> = self
            .known_devices
            .iter()
            .map(|entry| entry.value().clone())
            .collect();

        Ok(device_list)
    }

    async fn scan_device(&self, sender_address: &str, port: u16) -> Result<DeviceInfo> {
        debug!("扫描设备: {}:{}", sender_address, port);

        // 创建HTTP客户端
        let client = client::HttpClient::new()?;

        // 获取本地设备状态
        let local_status = self.get_device_status().await;

        // 使用POST方法发送本地设备状态并获取对方设备状态
        let device_status = client
            .check_status(sender_address, port, local_status)
            .await
            .map_err(|e| HiveDropError::NetworkError(format!("扫描设备失败: {}", e)))?;

        debug!(
            "发现设备: {} ({})",
            device_status.device_name, device_status.device_id
        );

        // 使用设备状态创建DeviceInfo
        let device_info = DeviceInfo {
            device_id: device_status.device_id.clone(),
            device_name: device_status.device_name.clone(),
            address: vec![sender_address.to_string()],
            port,
            is_online: device_status.is_online,
            version: device_status.version.clone(),
        };

        // 检查是否已经存在这个设备，并更新地址列表
        if let Some(mut existing_device) = self.known_devices.get_mut(&device_status.device_id) {
            // 如果地址列表中没有当前地址，则添加
            if !existing_device
                .address
                .contains(&sender_address.to_string())
            {
                existing_device.address.push(sender_address.to_string());
                debug!(
                    "为设备 {} 添加新地址: {}",
                    existing_device.device_id, sender_address
                );
            }
            // 更新在线状态和其他信息
            existing_device.is_online = device_status.is_online;
            existing_device.version = device_status.version.clone();
            existing_device.device_name = device_status.device_name.clone();

            // 复制一份用于事件发送
            let updated_device = existing_device.clone();

            // 发送设备更新事件
            self.send_event(TransferEvent::DeviceUpdated(updated_device.clone()));

            // 返回更新后的设备
            Ok(updated_device)
        } else {
            // 添加新设备
            debug!(
                "添加新设备: {} ({})",
                device_info.device_name, device_info.device_id
            );
            self.known_devices
                .insert(device_status.device_id.clone(), device_info.clone());

            // 发送新设备发现事件
            self.send_event(TransferEvent::DeviceDiscovered(device_info.clone()));

            Ok(device_info)
        }
    }

    async fn get_pending_send_files(&self) -> Result<Vec<PendingSendFileInfo>> {
        let files: Vec<PendingSendFileInfo> = self
            .sent_files
            .iter()
            .map(|entry| entry.value().clone())
            .collect();

        Ok(files)
    }

    async fn get_pending_receive_files(&self) -> Result<Vec<PendingReceiveFileInfo>> {
        let files: Vec<PendingReceiveFileInfo> = self
            .received_files
            .iter()
            .map(|entry| entry.value().clone())
            .collect();

        Ok(files)
    }

    // 实现as_any方法
    fn as_any(&self) -> &dyn Any {
        self
    }
}
