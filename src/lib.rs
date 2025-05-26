pub mod dto;
pub mod error;
pub mod http;
pub mod types;
pub mod utils;

use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::broadcast;

use crate::dto::request::*;
use crate::dto::response::*;
use crate::types::*;

use crate::error::Result;
use crate::http::HttpTransferService;

/// 传输服务特性 - 定义文件传输功能的核心接口
#[async_trait]
pub trait TransferService: Sync + Send {
    /// 通过路径创建传输请求 发送方->接收方
    ///
    /// # 参数
    /// * `file_path` - 要传输的文件路径
    /// * `device_id` - 接收方设备ID
    ///
    /// # 返回
    /// * `Result<TransferRequest>` - 创建的传输请求或错误
    async fn create_transfer_request_by_path(
        &self,
        file_path: &str,
        device_id: &str,
    ) -> Result<TransferRequestResponse>;

    /// 通过多个路径创建传输请求 发送方->接收方
    ///
    /// # 参数
    /// * `file_paths` - 要传输的多个文件路径列表
    /// * `device_id` - 接收方设备ID
    /// * `excluded_patterns` - 可选的排除模式列表
    ///
    /// # 返回
    /// * `Result<TransferRequestResponse>` - 创建的传输请求或错误
    async fn create_transfer_request_by_paths(
        &self,
        file_paths: Vec<String>,
        device_id: &str,
        excluded_patterns: Option<Vec<String>>,
    ) -> Result<TransferRequestResponse>;

    /// 接受传输请求 接收方->发送方(并开始传输)
    ///
    /// # 参数
    /// * `request_id` - 请求ID
    /// * `save_path` - 保存路径
    /// * `exclusion_list` - 排除的文件列表(file_id)
    ///
    /// # 返回
    /// * `Result<()>` - 操作成功或错误
    async fn accept_transfer_request(
        &self,
        request_id: &str,
        save_path: &str,
        exclusion_list: Vec<String>,
    ) -> Result<()>;

    /// 拒绝传输请求 接收方->发送方
    ///
    /// # 参数
    /// * `request_id` - 请求ID
    /// * `reason` - 拒绝原因（可选）
    ///
    /// # 返回
    /// * `Result<()>` - 操作成功或错误
    async fn reject_transfer_request(&self, request_id: &str, reason: Option<String>)
    -> Result<()>;

    /// 取消传输请求 发送方->接收方
    async fn cancel_transfer_request(&self, request_id: &str) -> Result<()>;

    /// 获取所有接收到的传输请求(接收方)
    /// 接收方在此列表中找到待处理请求, 并可以选择接受或拒绝
    /// # 返回
    /// * `Result<Vec<TransferRequest>>` - 待处理请求列表或错误
    async fn get_received_requests(&self) -> Result<Vec<TransferRequest>>;

    /// 获取所有已发送请求的传输请求(发送方)
    /// 发送方在请求向接收方发送后, 会在此列表中找到请求
    /// # 返回
    /// * `Result<Vec<TransferRequest>>` - 已发送请求列表或错误
    async fn get_sent_requests(&self) -> Result<Vec<TransferRequest>>;

    /// 暂停文件下载(仅对接收方有效)
    ///
    /// # 参数
    /// * `request_id` - 请求ID
    /// * `file_id` - 文件ID
    ///
    /// # 返回
    /// * `Result<()>` - 操作成功或错误
    async fn pause_file_download(&self, request_id: &str, file_id: &str) -> Result<()>;

    /// 恢复文件下载(仅对接收方有效)
    ///
    /// # 参数
    /// * `request_id` - 请求ID
    /// * `file_id` - 文件ID
    ///
    /// # 返回
    /// * `Result<()>` - 操作成功或错误
    async fn resume_file_download(&self, request_id: &str, file_id: &str) -> Result<()>;

    /// 取消文件接收(仅对接收方有效)
    ///
    /// # 参数
    /// * `request_id` - 请求ID
    /// * `reason` - 取消原因
    ///
    /// # 返回
    /// * `Result<()>` - 操作成功或错误
    async fn cancel_file_receiving(&self, request_id: &str, reason: &str) -> Result<()>;

    /// 取消文件发送(仅对发送方有效)
    /// 发送方取消传输时, 会发送取消请求给接收方
    /// # 参数
    /// * `request_id` - 请求ID
    /// * `reason` - 取消原因
    async fn cancel_file_send(&self, request_id: &str, reason: &str) -> Result<()>;

    /// 获取当前发现等级
    ///
    /// # 返回
    /// * `DiscoveryLevel` - 当前发现等级
    fn get_discovery_level(&self) -> DiscoveryLevel;

    /// 设置发现等级
    ///
    /// # 参数
    /// * `level` - 要设置的发现等级
    ///
    /// # 返回
    /// * `Result<()>` - 操作成功或错误
    async fn set_discovery_level(&self, level: DiscoveryLevel) -> Result<()>;

    /// 订阅传输事件
    ///
    /// # 返回
    /// * `broadcast::Receiver<TransferEvent>` - 事件接收器
    fn subscribe(&self) -> broadcast::Receiver<TransferEvent>;

    /// 启动传输服务
    ///
    /// # 返回
    /// * `Result<()>` - 操作成功或错误
    async fn start(&self) -> Result<()>;

    /// 停止传输服务
    ///
    /// # 返回
    /// * `Result<()>` - 操作成功或错误
    async fn stop(&self) -> Result<()>;

    /// 关闭传输服务并释放资源
    ///
    /// # 返回
    /// * `Result<()>` - 操作成功或错误
    async fn shutdown(&self) -> Result<()>;

    /// 重试失败的传输
    ///
    /// # 参数
    /// * `request_id` - 请求ID
    /// * `file_id` - 文件ID
    ///
    /// # 返回
    /// * `Result<()>` - 操作成功或错误
    async fn retry_failed_transfer(&self, request_id: &str, file_id: &str) -> Result<()>;

    /// 返回已知设备列表
    ///
    /// # 返回
    /// * `Result<Vec<DeviceInfo>>` - 附近设备列表或错误
    async fn get_known_devices(&self) -> Result<Vec<DeviceInfo>>;

    /// 扫描指定地址的设备,如果成功,就添加设备到已知设备列表
    ///
    /// # 参数
    /// * `sender_address` - 设备ip
    /// * `port` - 设备端口
    ///
    /// # 返回
    /// * `Result<Vec<DeviceInfo>>` - 对方设备信息或错误
    async fn scan_device(&self, sender_address: &str, port: u16) -> Result<DeviceInfo>;

    /// 获取待发送文件列表
    ///
    /// # 返回
    /// * `Result<Vec<PendingSendFileInfo>>` - 待发送文件列表或错误
    async fn get_pending_send_files(&self) -> Result<Vec<PendingSendFileInfo>>;

    /// 获取待接收文件列表
    ///
    /// # 返回
    /// * `Result<Vec<PendingReceiveFileInfo>>` - 待接收文件列表或错误  
    async fn get_pending_receive_files(&self) -> Result<Vec<PendingReceiveFileInfo>>;

    /// 将特征对象转换为任意类型（用于调试）
    fn as_any(&self) -> &dyn std::any::Any;
}

/// 创建传输服务实例
///
/// # 参数
/// * `implementation` - 传输实现类型
/// * `config` - 服务配置
///
/// # 返回
/// * `Result<Arc<dyn TransferService>>` - 传输服务实例或错误
pub async fn create_transfer_service(
    implementation: TransferImplementation,
    config: TransferServiceConfig,
) -> Result<Arc<dyn TransferService>> {
    match implementation {
        TransferImplementation::Http => Ok(HttpTransferService::create(config).await),
    }
}
