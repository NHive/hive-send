use serde::{Deserialize, Serialize};

/// 发现等级枚举 - 控制设备在网络中的可见性
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum DiscoveryLevel {
    /// 设备不可被发现
    Hidden,
    /// 设备可被附近所有人发现
    AllNearby,
    /// 设备仅可被用户本人的其他设备发现
    UserOnly,
}

impl std::fmt::Display for DiscoveryLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DiscoveryLevel::Hidden => write!(f, "Hidden"),
            DiscoveryLevel::AllNearby => write!(f, "AllNearby"),
            DiscoveryLevel::UserOnly => write!(f, "UserOnly"),
        }
    }
}

/// 实现方:(发送方, 接收方)
/// 用于响应服务状态检查
/// 状态检查响应 - GET /api/status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceStatusInfo {
    /// 设备ID
    pub device_id: String,
    /// 设备发现等级
    pub discovery_level: DiscoveryLevel,
    /// 设备在线状态
    pub is_online: bool,
    /// 设备应用版本
    pub version: String,
    /// 服务端口
    pub port: u16,
}

/// 实现方:(接收方)
/// 发送方请求文件发送, 接收方响应
/// 传输请求响应 - POST /api/transfer/request 的响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferRequestResponse {
    /// 请求ID
    pub request_id: String,
    /// 请求状态："pending"表示等待处理
    pub status: String,
}

/// 实现方:(接收方)
/// 发送方主动查询传输请求状态, 接收方响应
/// 传输请求状态响应 - GET /api/transfer/request/{request_id}/status 的响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferStatusResponse {
    /// 请求ID
    pub request_id: String,
    /// 当前状态：accepted（已接受）、rejected（已拒绝）或pending（等待处理）
    pub status: String,
    /// 可选消息
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// 实现方:(发送方)
/// 接收方在下载文件前, 先请求文件的元数据信息, 用于获取文件大小、哈希值等
/// 文件元数据信息（用于HEAD请求的响应头）
#[derive(Debug, Clone)]
pub struct FileMetadata {
    /// 文件大小（字节）
    pub content_length: u64,
    /// MIME类型
    pub content_type: String,
    /// 文件名
    pub filename: String,
}

#[derive(Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub message: Option<String>,
}

impl<T> ApiResponse<T> {
    pub fn error(message: String) -> Self {
        Self {
            success: false,
            data: None,
            message: Some(message),
        }
    }
}
