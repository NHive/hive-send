use serde::{Deserialize, Serialize};

use crate::types;

/// 实现方:(发送方)
/// 文件传输请求中的单个文件信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    pub file_id: String,   // 文件唯一标识符
    pub file_name: String, // 文件名
    pub file_path: String, // 相对路径
    pub file_size: u64,    // 文件大小（字节）
    pub mime_type: String, // MIME类型
    pub is_dir: bool,      // 是否是目录
}

/// 实现方:(发送方)
/// 发送方发起传输请求
/// 传输请求 - POST /api/transfer/request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferRequest {
    /// 请求唯一标识符
    pub request_id: String,
    /// 发送方设备ID
    pub sender_device_id: String,
    /// 接收方设备ID
    pub receiver_device_id: String,
    /// 要传输的文件列表
    pub files: Vec<FileInfo>,
    /// 请求状态
    pub status: String,
}

/// 实现方:(接收方)
/// 接收方发请求给发送方, 通知接收方同意或拒绝传输请求
/// 传输响应 - POST /api/transfer/response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferResponse {
    /// 对应请求的ID
    pub request_id: String,
    /// 响应状态：accepted（接受）或rejected（拒绝）
    pub status: String,
    /// 接收方设备ID
    pub receiver_device_id: String,
}

/// 实现方:(接收方)
/// 接收方主动将文件的传输进度发送给发送方
/// 传输进度报告 - POST /api/transfer/progress
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferProgress {
    /// 对应请求的ID
    pub request_id: String,
    /// 文件ID
    pub file_id: String,
    /// 已接收字节数
    pub bytes_received: u64,
    /// 传输状态
    pub status: types::TransferStatus,
    /// 可选的错误信息
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    /// 传输速度（字节/秒）
    #[serde(default)]
    pub speed: u64,
}

/// 实现方:(发送方, 接收方)
/// 发送方或者接收方,任意一方取消传输时, 发送取消请求给对方
/// 取消传输请求 - POST /api/transfer/cancel
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferCancel {
    /// 要取消的传输请求ID
    pub request_id: String,
    /// 取消原因：user_cancelled（用户取消）、error（错误）或timeout（超时）
    pub reason: String,
}
