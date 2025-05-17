use reqwest::StatusCode;

use crate::dto::request::*;
use crate::dto::response::*;
use crate::error::{HiveDropError, Result};
use crate::http::client::HttpClient;

impl HttpClient {
    /// 发送传输请求
    pub async fn send_transfer_request(
        &self,
        request: &TransferRequest,
        receiver_address: &str,
        port: u16,
    ) -> Result<TransferRequestResponse> {
        let url = format!("https://{}:{}/api/transfer/request", receiver_address, port);

        let response = self
            .client
            .post(&url)
            .json(request)
            .send()
            .await
            .map_err(|e| HiveDropError::NetworkError(format!("传输请求发送失败: {}", e)))?;

        if response.status() == StatusCode::OK {
            let response_data = response
                .json::<TransferRequestResponse>()
                .await
                .map_err(|e| HiveDropError::NetworkError(format!("解析传输请求响应失败: {}", e)))?;

            Ok(response_data)
        } else {
            Err(HiveDropError::NetworkError(format!(
                "传输请求失败，状态码: {}",
                response.status()
            )))
        }
    }

    /// 查询传输请求状态
    pub async fn check_transfer_status(
        &self,
        request_id: &str,
        receiver_address: &str,
        port: u16,
    ) -> Result<TransferStatusResponse> {
        let url = format!(
            "https://{}:{}/api/transfer/request/{}/status",
            receiver_address, port, request_id
        );

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| HiveDropError::NetworkError(format!("状态查询请求失败: {}", e)))?;

        if response.status() == StatusCode::OK {
            let status = response
                .json::<TransferStatusResponse>()
                .await
                .map_err(|e| HiveDropError::NetworkError(format!("解析状态查询响应失败: {}", e)))?;

            Ok(status)
        } else {
            Err(HiveDropError::NetworkError(format!(
                "状态查询失败，状态码: {}",
                response.status()
            )))
        }
    }

    /// 发送传输响应（接受或拒绝）
    pub async fn send_transfer_response(
        &self,
        response: &TransferResponse,
        sender_address: &str,
        port: u16,
    ) -> Result<()> {
        let url = format!("https://{}:{}/api/transfer/response", sender_address, port);

        let http_response = self
            .client
            .post(&url)
            .json(response)
            .send()
            .await
            .map_err(|e| HiveDropError::NetworkError(format!("传输响应发送失败: {}", e)))?;

        if http_response.status() == StatusCode::OK {
            Ok(())
        } else {
            Err(HiveDropError::NetworkError(format!(
                "传输响应失败，状态码: {}",
                http_response.status()
            )))
        }
    }

    /// 报告传输进度
    pub async fn report_progress(
        &self,
        progress: &TransferProgress,
        sender_address: &str,
        port: u16,
    ) -> Result<()> {
        let url = format!("https://{}:{}/api/transfer/progress", sender_address, port);

        let response = self
            .client
            .post(&url)
            .json(progress)
            .send()
            .await
            .map_err(|e| HiveDropError::NetworkError(format!("进度报告发送失败: {}", e)))?;

        if response.status() == StatusCode::OK {
            Ok(())
        } else {
            Err(HiveDropError::NetworkError(format!(
                "进度报告失败，状态码: {}",
                response.status()
            )))
        }
    }

    /// 取消传输
    pub async fn cancel_transfer(
        &self,
        cancel: &TransferCancel,
        peer_address: &str,
        port: u16,
    ) -> Result<()> {
        let url = format!("https://{}:{}/api/transfer/cancel", peer_address, port);

        let response = self
            .client
            .post(&url)
            .json(cancel)
            .send()
            .await
            .map_err(|e| HiveDropError::NetworkError(format!("取消传输请求发送失败: {}", e)))?;

        if response.status() == StatusCode::OK {
            Ok(())
        } else {
            Err(HiveDropError::NetworkError(format!(
                "取消传输失败，状态码: {}",
                response.status()
            )))
        }
    }

    /// 验证文件
    pub async fn verify_file(
        &self,
        verify: &TransferVerify,
        sender_address: &str,
        port: u16,
    ) -> Result<VerifyResponse> {
        let url = format!("https://{}:{}/api/transfer/verify", sender_address, port);

        let response = self
            .client
            .post(&url)
            .json(verify)
            .send()
            .await
            .map_err(|e| HiveDropError::NetworkError(format!("文件验证请求发送失败: {}", e)))?;

        if response.status() == StatusCode::OK {
            let verify_response = response
                .json::<VerifyResponse>()
                .await
                .map_err(|e| HiveDropError::NetworkError(format!("解析文件验证响应失败: {}", e)))?;

            Ok(verify_response)
        } else {
            Err(HiveDropError::NetworkError(format!(
                "文件验证失败，状态码: {}",
                response.status()
            )))
        }
    }
}
