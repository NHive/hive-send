use reqwest::StatusCode;

use crate::error::{HiveDropError, Result};
use crate::dto::response::DeviceStatusInfo;
use crate::http::client::HttpClient;

impl HttpClient {
    /// 检查设备状态
    pub async fn check_status(&self, device_address: &str, port: u16) -> Result<DeviceStatusInfo> {
        let url = format!("https://{}:{}/api/status", device_address, port);
        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| HiveDropError::NetworkError(format!("状态检查请求失败: {}", e)))?;

        if response.status() == StatusCode::OK {
            let status_info = response
                .json::<DeviceStatusInfo>()
                .await
                .map_err(|e| HiveDropError::NetworkError(format!("解析状态响应失败: {}", e)))?;

            Ok(status_info)
        } else {
            Err(HiveDropError::NetworkError(format!(
                "状态检查失败，状态码: {}",
                response.status()
            )))
        }
    }
}
