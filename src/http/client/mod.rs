mod file_client;
mod status_client;
mod transfer_client;

use reqwest::{Client, ClientBuilder};
use std::time::Duration;

use crate::error::{HiveDropError, Result};

/// HTTP 客户端 - 用于与其他设备通信
pub struct HttpClient {
    pub client: Client,
}

impl HttpClient {
    /// 创建新的 HTTP 客户端
    pub fn new() -> Result<Self> {
        // 配置客户端，启用自签名 TLS 证书支持
        let client = ClientBuilder::new()
            .danger_accept_invalid_certs(true) // 允许自签名证书
            .timeout(Duration::from_secs(1))  // 设置超时时间
            .build()
            .map_err(|e| HiveDropError::NetworkError(format!("创建 HTTP 客户端失败: {}", e)))?;

        Ok(Self { client })
    }
}
