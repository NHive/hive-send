use reqwest::StatusCode;

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

        println!("请求文件元数据: {}", url);

        let mut request = self.client.get(&url);

        // 如果指定了范围，添加Range头
        if let Some((start, end)) = range {
            request = request.header("Range", format!("bytes={}-{}", start, end));
        }

        let response = request
            .send()
            .await
            .map_err(|e| HiveDropError::NetworkError(format!("文件下载请求失败: {}", e)))?;

        if response.status() == StatusCode::OK || response.status() == StatusCode::PARTIAL_CONTENT {
            let bytes = response
                .bytes()
                .await
                .map_err(|e| HiveDropError::NetworkError(format!("读取文件数据失败: {}", e)))?
                .to_vec();

            Ok(bytes)
        } else {
            Err(HiveDropError::NetworkError(format!(
                "文件下载失败，状态码: {}",
                response.status()
            )))
        }
    }
}
