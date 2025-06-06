use reqwest::StatusCode;
use std::time::Duration;

use crate::dto::response::FileMetadata;
use crate::error::{HiveDropError, Result};
use crate::http::client::HttpClient;

impl HttpClient {
    /// 获取文件元数据
    pub async fn get_file_metadata(
        &self,
        file_id: &str,
        sender_address: &str,
        port: u16,
    ) -> Result<FileMetadata> {
        let url = format!("https://{}:{}/api/file/{}", sender_address, port, file_id);

        println!("请求文件元数据: {}", url);

        let response = self
            .client
            .head(&url)
            .send()
            .await
            .map_err(|e| HiveDropError::NetworkError(format!("文件元数据请求失败: {}", e)))?;

        if response.status() == StatusCode::OK {
            let headers = response.headers();

            // 解析响应头
            let content_length = headers
                .get("content-length")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok())
                .ok_or_else(|| HiveDropError::NetworkError("缺少Content-Length头".to_string()))?;

            let content_type = headers
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("application/octet-stream")
                .to_string();

            // 从Content-Disposition头中提取文件名
            let filename = headers
                .get("content-disposition")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| {
                    // 解析 "attachment; filename="example.txt"" 格式
                    let parts: Vec<&str> = v.split(';').collect();
                    for part in parts {
                        let part = part.trim();
                        if part.starts_with("filename=") {
                            // 提取文件名并去除引号
                            return Some(
                                part.strip_prefix("filename=")
                                    .unwrap_or("")
                                    .trim_matches('"')
                                    .to_string(),
                            );
                        }
                    }
                    None
                })
                .ok_or_else(|| {
                    HiveDropError::NetworkError(
                        "无法从Content-Disposition头中提取文件名".to_string(),
                    )
                })?;

            Ok(FileMetadata {
                content_length,
                content_type,
                filename,
            })
        } else {
            Err(HiveDropError::NetworkError(format!(
                "获取文件元数据失败，状态码: {}",
                response.status()
            )))
        }
    }

    /// 下载文件分片
    pub async fn download_file_chunk(
        &self,
        file_id: &str,
        range: Option<(u64, u64)>,
        sender_address: &str,
        port: u16,
    ) -> Result<Vec<u8>> {
        let url = format!("https://{}:{}/api/file/{}", sender_address, port, file_id);

        println!("下载文件分片: {}", url);

        let mut request = self.client.get(&url);

        // 如果指定了范围，添加Range头
        if let Some((start, end)) = range {
            request = request.header("Range", format!("bytes={}-{}", start, end));
            println!("请求范围: bytes={}-{}", start, end);
        }

        // 添加更多HTTP头确保兼容性
        request = request
            .header("Accept", "application/octet-stream, */*")
            .header("User-Agent", "HiveDrop/1.0")
            .header("Connection", "keep-alive")
            .timeout(Duration::from_secs(60));

        let response = request
            .send()
            .await
            .map_err(|e| {
                println!("HTTP请求失败: {}", e);
                HiveDropError::NetworkError(format!("文件下载请求失败: {}", e))
            })?;

        println!("响应状态码: {}", response.status());
        println!("响应头: {:?}", response.headers());

        if response.status() == StatusCode::OK || response.status() == StatusCode::PARTIAL_CONTENT {
            // 获取内容长度用于验证
            let content_length = response.headers()
                .get("content-length")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok());
            
            if let Some(length) = content_length {
                println!("期望接收数据长度: {} 字节", length);
            }

            // 分块读取响应体以避免内存问题
            let bytes = response
                .bytes()
                .await
                .map_err(|e| {
                    println!("读取响应体失败: {}", e);
                    // 提供更详细的错误信息
                    HiveDropError::NetworkError(format!(
                        "读取文件数据失败: {} (可能是网络中断或服务器响应格式错误)", e
                    ))
                })?
                .to_vec();

            println!("实际接收数据长度: {} 字节", bytes.len());

            // 验证接收的数据长度
            if let Some(expected_length) = content_length {
                if bytes.len() as u64 != expected_length {
                    return Err(HiveDropError::NetworkError(format!(
                        "数据长度不匹配: 期望 {} 字节，实际接收 {} 字节",
                        expected_length, bytes.len()
                    )));
                }
            }

            Ok(bytes)
        } else {
            Err(HiveDropError::NetworkError(format!(
                "文件下载失败，状态码: {}, 响应: {:?}",
                response.status(),
                response.text().await.unwrap_or_else(|_| "无法读取响应内容".to_string())
            )))
        }
    }
}
