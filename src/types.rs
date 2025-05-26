use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::dto::request::*;
use crate::dto::response::*;

/// 传输事件枚举 - 用于在系统内部组件之间通知传输状态变化
#[derive(Clone, Debug)]
pub enum TransferEvent {
    /// 传输服务已启动
    ServiceStarted,

    /// 传输服务已停止
    ServiceStopped,

    /// 新的传输请求已收到
    RequestReceived(TransferRequest),

    /// 请求状态已发生变化（如从pending到accepted）
    RequestStatusChanged {
        request_id: String,
        status: String,
        message: Option<String>,
    },

    /// 传输进度已更新
    ProgressUpdated(TransferProgress),

    /// 传输已成功完成
    TransferCompleted {
        request_id: String,
        file_id: String,
        save_path: Option<String>,
    },

    /// 传输失败
    TransferFailed {
        request_id: String,
        file_id: String,
        error: String,
    },

    /// 传输被取消
    TransferCancelled { request_id: String, reason: String },

    /// 文件验证结果
    VerificationResult {
        request_id: String,
        file_id: String,
        is_valid: bool,
        original_hash: String,
    },

    /// 设备状态变更
    DeviceStatusChanged { device_id: String, is_online: bool },

    /// 发现等级变更
    DiscoveryLevelChanged {
        device_id: String,
        level: DiscoveryLevel,
    },

    /// 设备状态更新
    DeviceUpdated(DeviceInfo),

    /// 发现新设备
    DeviceDiscovered(DeviceInfo),

    /// 额外信息已更改
    ExtraInfoChanged {
        device_id: String,
        extra_info: Option<Value>,
    },
}

/// 传输状态枚举 - 表示单个文件传输的状态
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum TransferStatus {
    Pending,    // 等待中
    Accepted,   // 已接受
    Rejected,   // 已拒绝
    InProgress, // 传输中
    Paused,     // 已暂停
    Completed,  // 已完成
    Failed,     // 失败
    Cancelled,  // 已取消
}

/// 设备信息结构体 - 用于设备发现功能
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    /// 设备ID
    pub device_id: String,
    /// 设备地址
    pub address: Vec<String>,
    /// 设备端口
    pub port: u16,
    /// 设备在线状态
    pub is_online: bool,
    /// 设备应用版本
    pub version: String,
    /// 设备额外信息
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra_info: Option<Value>,
}

/// 传输服务实现类型
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TransferImplementation {
    Http, // HTTP传输
}

/// 传输服务配置
#[derive(Debug, Clone)]
pub struct TransferServiceConfig {
    /// 本地设备ID
    pub device_id: String,
    /// 服务器监听端口
    pub server_port: u16,
    /// 最大并发传输数
    pub max_concurrent_transfers: usize,
    /// 文件传输分块大小(字节)
    pub chunk_size: usize,
    /// 应用程序版本
    pub version: String,
    /// 加密密钥路径
    pub key_path: Option<String>,
    /// 传输超时时间(秒)
    pub transfer_timeout: u64,
    /// 连接重试次数
    pub retry_attempts: u32,
    /// 重试间隔(毫秒)
    pub retry_interval: u64,
    /// 设备发现等级
    pub discovery_level: DiscoveryLevel,
}

impl Default for TransferServiceConfig {
    fn default() -> Self {
        Self {
            device_id: uuid::Uuid::new_v4().to_string(),
            server_port: 61314,
            max_concurrent_transfers: 10,
            chunk_size: 10 * 1024 * 1024, // 10MB
            version: env!("CARGO_PKG_VERSION").to_string(),
            key_path: None,
            transfer_timeout: 300, // 5分钟
            retry_attempts: 3,
            retry_interval: 1000,                    // 1秒
            discovery_level: DiscoveryLevel::Hidden, // 默认不可被发现
        }
    }
}

/// 待发送文件信息
/// 待发送文件的单个文件信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingSendFileInfo {
    pub file_id: String,            // 文件唯一标识符
    pub file_name: String,          // 文件名
    pub file_relative_path: String, // 相对路径
    pub file_absolute_path: String, // 绝对路径
    pub file_size: u64,             // 文件大小（字节）
    pub mime_type: String,          // MIME类型
    pub is_dir: bool,               // 是否是目录
    pub progress: u64,              // 传输进度（字节）
}

impl PendingSendFileInfo {
    /// 将PendingSendFileInfo转换为FileInfo
    pub fn to_file_info(&self) -> FileInfo {
        FileInfo {
            file_id: self.file_id.clone(),
            file_name: self.file_name.clone(),
            file_path: self.file_relative_path.clone(),
            file_size: self.file_size,
            mime_type: self.mime_type.clone(),
            is_dir: self.is_dir,
        }
    }
}

/// 待接收文件信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingReceiveFileInfo {
    pub request_id: String,         // 传输请求ID
    pub device_id: String,          // 发送方设备ID
    pub file_id: String,            // 文件唯一标识符
    pub file_name: String,          // 文件名
    pub file_relative_path: String, // 相对路径
    pub file_absolute_path: String, // 绝对路径
    pub file_size: u64,             // 文件大小（字节）
    pub mime_type: String,          // MIME类型
    pub is_dir: bool,               // 是否是目录
    pub progress: u64,              // 传输进度（字节）
    pub status: TransferStatus,     // 传输状态
}
