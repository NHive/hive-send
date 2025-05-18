use axum::Json;
use axum::{
    Router,
    body::Body,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, head},
};
use axum_extra::{TypedHeader, headers::Range};
use log::{debug, error};
use std::io::SeekFrom;
use std::ops::Bound;
use std::path::Path as FilePath;
use std::sync::Arc;
use tokio::fs::File as TokioFile;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::io::ReaderStream;

use crate::dto::response::ApiResponse;
use crate::http::HttpTransferService;

/// 处理文件元数据请求 (HEAD 请求)
async fn handle_file_metadata(
    State(service): State<Arc<HttpTransferService>>,
    Path(file_id): Path<String>,
) -> Response {
    debug!("处理文件元数据请求: {}", file_id);

    // 从sent_files缓存中获取文件信息
    if let Some(file_info) = service.sent_files.get(&file_id) {
        // 构建元数据响应
        let mut headers = HeaderMap::new();

        headers.insert(
            header::CONTENT_LENGTH,
            file_info.file_size.to_string().parse().unwrap(),
        );

        headers.insert(header::CONTENT_TYPE, file_info.mime_type.parse().unwrap());

        // 设置文件名
        let filename = format!("attachment; filename=\"{}\"", file_info.file_name);
        headers.insert(header::CONTENT_DISPOSITION, filename.parse().unwrap());

        // 设置接受范围请求
        headers.insert(header::ACCEPT_RANGES, "bytes".parse().unwrap());

        debug!(
            "文件元数据请求成功: {}, 大小: {}",
            file_id, file_info.file_size
        );

        (StatusCode::OK, headers, "").into_response()
    } else {
        error!("文件不存在: {}", file_id);
        (
            StatusCode::NOT_FOUND,
            Json(ApiResponse::<()>::error(format!("文件不存在: {}", file_id))),
        )
            .into_response()
    }
}

/// 处理文件下载请求
async fn handle_file_download(
    State(service): State<Arc<HttpTransferService>>,
    Path(file_id): Path<String>,
    range_header: Option<TypedHeader<Range>>,
) -> Response {
    debug!("处理文件下载请求: {}", file_id);

    // 从sent_files缓存中获取文件信息
    if let Some(file_info) = service.sent_files.get(&file_id) {
        // 构建文件路径
        let file_path = FilePath::new(&file_info.file_absolute_path);

        // 检查文件是否存在
        match tokio::fs::metadata(&file_path).await {
            Ok(metadata) => {
                let file_size = metadata.len();

                // 处理Range请求 (分片下载)
                if let Some(TypedHeader(range)) = range_header {
                    debug!("收到Range请求: {:?}", range);

                    // 从Range头获取范围
                    let ranges = range.satisfiable_ranges(file_size).collect::<Vec<_>>();

                    if ranges.is_empty() {
                        return (
                            StatusCode::RANGE_NOT_SATISFIABLE,
                            Json(ApiResponse::<()>::error("无效的Range请求".to_string())),
                        )
                            .into_response();
                    }

                    // 目前只处理第一个范围
                    let (start_bound, end_bound) = ranges[0];

                    // 从Bound<u64>提取实际的u64值
                    let start = match start_bound {
                        Bound::Included(s) => s,
                        Bound::Excluded(s) => s + 1,
                        Bound::Unbounded => 0,
                    };

                    let end = match end_bound {
                        Bound::Included(e) => e,
                        Bound::Excluded(e) => e - 1,
                        Bound::Unbounded => file_size - 1,
                    };

                    let bytes_to_read = end - start + 1;
                    debug!(
                        "分片下载: 开始={}, 结束={}, 总字节数={}",
                        start, end, bytes_to_read
                    );

                    // 使用异步文件 I/O
                    match TokioFile::open(file_path).await {
                        Ok(mut file) => {
                            if let Err(e) = file.seek(SeekFrom::Start(start)).await {
                                error!("设置文件位置失败: {}", e);
                                return (
                                    StatusCode::INTERNAL_SERVER_ERROR,
                                    Json(ApiResponse::<()>::error("文件读取错误".to_string())),
                                )
                                    .into_response();
                            }

                            // 创建限制大小的异步读取器，读取指定范围的数据
                            let mut buffer = vec![0u8; bytes_to_read as usize];
                            match file.read_exact(&mut buffer).await {
                                Ok(_) => {
                                    // 构建响应头
                                    let mut headers = HeaderMap::new();
                                    headers.insert(
                                        header::CONTENT_TYPE,
                                        file_info.mime_type.parse().unwrap(),
                                    );

                                    headers.insert(
                                        header::CONTENT_RANGE,
                                        format!("bytes {}-{}/{}", start, end, file_size)
                                            .parse()
                                            .unwrap(),
                                    );

                                    headers.insert(
                                        header::CONTENT_LENGTH,
                                        bytes_to_read.to_string().parse().unwrap(),
                                    );

                                    headers.insert(header::ACCEPT_RANGES, "bytes".parse().unwrap());

                                    // 设置文件名
                                    let filename =
                                        format!("attachment; filename=\"{}\"", file_info.file_name);
                                    headers.insert(
                                        header::CONTENT_DISPOSITION,
                                        filename.parse().unwrap(),
                                    );

                                    debug!("开始发送分片数据");
                                    (StatusCode::PARTIAL_CONTENT, headers, buffer).into_response()
                                }
                                Err(e) => {
                                    error!("读取文件数据失败: {}", e);
                                    (
                                        StatusCode::INTERNAL_SERVER_ERROR,
                                        Json(ApiResponse::<()>::error("文件读取错误".to_string())),
                                    )
                                        .into_response()
                                }
                            }
                        }
                        Err(e) => {
                            error!("无法打开文件: {}", e);
                            (
                                StatusCode::INTERNAL_SERVER_ERROR,
                                Json(ApiResponse::<()>::error("文件访问错误".to_string())),
                            )
                                .into_response()
                        }
                    }
                } else {
                    // 不是Range请求，返回整个文件
                    debug!("下载完整文件: {}", file_id);

                    match TokioFile::open(file_path).await {
                        Ok(file) => {
                            let stream = ReaderStream::new(file);

                            // 构建响应头
                            let mut headers = HeaderMap::new();
                            headers
                                .insert(header::CONTENT_TYPE, file_info.mime_type.parse().unwrap());

                            headers.insert(
                                header::CONTENT_LENGTH,
                                file_size.to_string().parse().unwrap(),
                            );

                            // 设置文件名
                            let filename =
                                format!("attachment; filename=\"{}\"", file_info.file_name);
                            headers.insert(header::CONTENT_DISPOSITION, filename.parse().unwrap());

                            headers.insert(header::ACCEPT_RANGES, "bytes".parse().unwrap());

                            debug!("开始发送完整文件");

                            // 直接使用 Axum 的 Body 类型，避免使用 StreamBody
                            let body = Body::from_stream(stream);
                            (StatusCode::OK, headers, body).into_response()
                        }
                        Err(e) => {
                            error!("无法打开文件: {}", e);
                            (
                                StatusCode::INTERNAL_SERVER_ERROR,
                                Json(ApiResponse::<()>::error("文件访问错误".to_string())),
                            )
                                .into_response()
                        }
                    }
                }
            }
            Err(e) => {
                error!("文件元数据获取失败: {}", e);
                (
                    StatusCode::NOT_FOUND,
                    Json(ApiResponse::<()>::error("文件不存在或无法访问".to_string())),
                )
                    .into_response()
            }
        }
    } else {
        error!("文件不存在: {}", file_id);
        (
            StatusCode::NOT_FOUND,
            Json(ApiResponse::<()>::error(format!("文件不存在: {}", file_id))),
        )
            .into_response()
    }
}

/// 注册文件处理相关路由
pub fn register() -> Router<Arc<HttpTransferService>> {
    Router::new()
        .route("/api/file/{file_id}", head(handle_file_metadata))
        .route("/api/file/{file_id}", get(handle_file_download))
}
