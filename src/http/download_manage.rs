use std::collections::VecDeque;
use std::io::SeekFrom;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use log::{debug, error, info, warn};
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tokio::sync::{Semaphore, mpsc};
use tokio::time::{interval, sleep};

use crate::TransferService;
use crate::dto::request::TransferProgress;
use crate::dto::response::FileMetadata;
use crate::error::{HiveDropError, Result};
use crate::http::HttpTransferService;
use crate::http::client::HttpClient;
use crate::types::{DeviceInfo, TransferStatus};

// 下载任务结构体
#[derive(Clone)]
pub struct DownloadTask {
    pub request_id: String,
    pub file_id: String,
    pub device_id: String,
    pub save_path: PathBuf,
    pub file_size: u64,
    pub is_dir: bool,
    pub start_offset: u64,
}

// 进度更新结构体
#[derive(Clone)]
struct ProgressUpdate {
    request_id: String,
    file_id: String,
    bytes_received: u64,
    status: TransferStatus,
    error_message: Option<String>,
    speed: u64, // 添加速度字段
}

// 速度统计结构体
#[derive(Clone)]
struct SpeedStat {
    file_id: String,
    bytes_last_second: u64,
    last_updated: Instant,
    bytes_history: VecDeque<u64>, // 保存最近几秒的字节数，用于计算平均速度
}

// 下载管理器
pub struct DownloadManager {
    service: Arc<HttpTransferService>,
    http_client: HttpClient,
    progress_sender: mpsc::Sender<ProgressUpdate>,
    semaphore: Arc<Semaphore>,
    // 记录活动下载的速度统计数据
    speed_stats: Arc<tokio::sync::Mutex<std::collections::HashMap<String, SpeedStat>>>,
}

impl DownloadManager {
    // 创建下载管理器
    fn new(service: Arc<HttpTransferService>) -> (Self, mpsc::Receiver<ProgressUpdate>) {
        let config = service.get_config();
        let (progress_sender, progress_receiver) = mpsc::channel(100);
        let semaphore = Arc::new(Semaphore::new(config.max_concurrent_transfers));

        // 创建HttpClient实例而不是reqwest Client
        let http_client = HttpClient::new().expect("创建HTTP客户端失败");

        (
            Self {
                service,
                http_client,
                progress_sender,
                semaphore,
                speed_stats: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
            },
            progress_receiver,
        )
    }

    // 开始下载任务
    async fn start_download(&self, task: DownloadTask) -> Result<()> {
        // 获取发送方设备信息
        let device_info = match self.service.get_device_info(&task.device_id).await {
            Some(info) => info,
            None => {
                return Err(HiveDropError::NotFoundError(format!(
                    "设备不存在: {}",
                    task.device_id
                )));
            }
        };

        // 记录保存路径的详细信息
        info!(
            "文件将保存到: {}, 路径是否存在: {}",
            task.save_path.display(),
            task.save_path.parent().map_or(false, |p| p.exists())
        );

        // 如果是目录，只需创建目录
        if task.is_dir {
            tokio::fs::create_dir_all(&task.save_path).await?;

            // 向发送方报告目录创建完成
            self.report_progress_to_sender(
                &task.request_id,
                &task.file_id,
                0,
                TransferStatus::Completed,
                None,
                0,
                &device_info,
            )
            .await?;

            self.update_progress(ProgressUpdate {
                request_id: task.request_id.clone(),
                file_id: task.file_id.clone(),
                bytes_received: 0,
                status: TransferStatus::Completed,
                error_message: None,
                speed: 0, // 下载完成时速度为0
            })
            .await?;
            info!("已成功创建目录: {}", task.save_path.display());

            // 从received_files中移除该目录
            self.service.received_files.remove(&task.file_id);

            return Ok(());
        }

        // 确保目标目录存在
        if let Some(parent) = task.save_path.parent() {
            if !parent.exists() {
                info!("创建父目录: {}", parent.display());
                tokio::fs::create_dir_all(parent).await?;
            }
        }

        // 尝试获取文件元数据
        let metadata = self.get_file_metadata(&device_info, &task).await?;

        // 初始化断点续传的偏移量
        let mut offset = task.start_offset;

        // 打开文件，准备写入或追加
        let mut file = if offset > 0 && task.save_path.exists() {
            info!("续传模式打开文件: {}", task.save_path.display());
            OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .open(&task.save_path)
                .await?
        } else {
            offset = 0; // 如果文件不存在，从头开始下载
            info!("新建文件: {}", task.save_path.display());
            File::create(&task.save_path).await?
        };

        // 如果有偏移量，将文件指针移动到指定位置
        if offset > 0 {
            file.seek(SeekFrom::Start(offset)).await?;
        }

        // 准备下载
        let chunk_size = self.service.get_config().chunk_size;

        // 发送初始进度更新
        self.update_progress(ProgressUpdate {
            request_id: task.request_id.clone(),
            file_id: task.file_id.clone(),
            bytes_received: offset,
            status: TransferStatus::InProgress,
            error_message: None,
            speed: 0, // 初始速度为0
        })
        .await?;

        // 下载进度变量
        let mut bytes_received = offset;
        let mut last_progress_update = Instant::now();
        let mut last_sender_report = Instant::now();
        let progress_interval = Duration::from_millis(500); // 每500毫秒更新一次本地进度
        let sender_report_interval = Duration::from_secs(2); // 每2秒向发送方报告一次进度

        // 初始化速度统计
        {
            let mut stats = self.speed_stats.lock().await;
            stats.insert(
                task.file_id.clone(),
                SpeedStat {
                    file_id: task.file_id.clone(),
                    bytes_last_second: 0,
                    last_updated: Instant::now(),
                    bytes_history: VecDeque::with_capacity(5), // 保存5秒的历史数据
                },
            );
        }

        // 使用分块下载
        while bytes_received < metadata.content_length {
            // 计算当前块的起始和结束位置
            let start = bytes_received;
            let end = (start + chunk_size as u64 - 1).min(metadata.content_length - 1);

            // 使用HttpClient的download_file_chunk方法下载文件分片
            let chunk = self
                .http_client
                .download_file_chunk(
                    &task.file_id,
                    Some((start, end)),
                    &device_info.address[0],
                    device_info.port,
                )
                .await?;

            // 写入数据到文件
            file.write_all(&chunk).await?;

            // 更新已接收的字节数
            let chunk_size = chunk.len() as u64;
            bytes_received += chunk_size;

            // 更新速度统计
            {
                let mut stats = self.speed_stats.lock().await;
                if let Some(stat) = stats.get_mut(&task.file_id) {
                    stat.bytes_last_second += chunk_size;
                }
            }

            // 定期更新进度
            if last_progress_update.elapsed() >= progress_interval {
                // 获取当前速度
                let current_speed = {
                    let stats = self.speed_stats.lock().await;
                    stats
                        .get(&task.file_id)
                        .map(|stat| {
                            if stat.bytes_history.is_empty() {
                                stat.bytes_last_second
                            } else {
                                stat.bytes_history.iter().sum::<u64>()
                                    / stat.bytes_history.len() as u64
                            }
                        })
                        .unwrap_or(0)
                };

                self.update_progress(ProgressUpdate {
                    request_id: task.request_id.clone(),
                    file_id: task.file_id.clone(),
                    bytes_received,
                    status: TransferStatus::InProgress,
                    error_message: None,
                    speed: current_speed, // 包含速度信息
                })
                .await?;
                last_progress_update = Instant::now();
            }

            // 定期向发送方报告进度
            if last_sender_report.elapsed() >= sender_report_interval {
                // 获取当前速度用于报告
                let current_speed = {
                    let stats = self.speed_stats.lock().await;
                    stats
                        .get(&task.file_id)
                        .map(|stat| {
                            if stat.bytes_history.is_empty() {
                                stat.bytes_last_second
                            } else {
                                stat.bytes_history.iter().sum::<u64>()
                                    / stat.bytes_history.len() as u64
                            }
                        })
                        .unwrap_or(0)
                };

                // 向发送方报告进度
                self.report_progress_to_sender(
                    &task.request_id,
                    &task.file_id,
                    bytes_received,
                    TransferStatus::InProgress,
                    None,
                    current_speed,
                    &device_info,
                )
                .await?;

                last_sender_report = Instant::now();
                debug!(
                    "已向发送方 {} 报告文件 {} 的下载进度: {} 字节，速度 {} 字节/秒",
                    device_info.device_id, task.file_id, bytes_received, current_speed
                );
            }
        }

        // 确保文件被完全写入
        file.flush().await?;
        drop(file); // 明确关闭文件句柄

        // 验证文件是否成功保存
        match tokio::fs::metadata(&task.save_path).await {
            Ok(metadata) => {
                info!(
                    "文件已成功保存: 路径={}, 大小={} 字节",
                    task.save_path.display(),
                    metadata.len()
                );
            }
            Err(e) => {
                error!(
                    "保存后文件访问失败: {}, 错误: {}",
                    task.save_path.display(),
                    e
                );
            }
        }

        // 清理速度统计数据
        {
            let mut stats = self.speed_stats.lock().await;
            stats.remove(&task.file_id);
        }

        // 向发送方报告下载完成
        self.report_progress_to_sender(
            &task.request_id,
            &task.file_id,
            bytes_received,
            TransferStatus::Completed,
            None,
            0,
            &device_info,
        )
        .await?;

        // 发送最终进度更新
        self.update_progress(ProgressUpdate {
            request_id: task.request_id.clone(),
            file_id: task.file_id.clone(),
            bytes_received,
            status: TransferStatus::Completed,
            error_message: None,
            speed: 0, // 下载完成时速度为0
        })
        .await?;

        // 从received_files中移除已完成的文件
        self.service.received_files.remove(&task.file_id);
        info!("文件 {} 已完成下载,从待接收列表中移除", task.file_id);

        Ok(())
    }

    // 获取文件元数据
    async fn get_file_metadata(
        &self,
        device_info: &DeviceInfo,
        task: &DownloadTask,
    ) -> Result<FileMetadata> {
        // 使用HttpClient的get_file_metadata方法获取文件元数据
        let metadata = self
            .http_client
            .get_file_metadata(&task.file_id, &device_info.address[0], device_info.port)
            .await?;

        Ok(metadata)
    }

    // 处理下载任务队列
    async fn process_download_queue(self: Arc<Self>) -> Result<()> {
        info!("下载队列处理器已启动");

        // 启动每秒更新速度统计数据的任务
        let speed_reporter = Arc::clone(&self);
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(1));
            loop {
                interval.tick().await;

                // 获取并更新每个下载的速度
                let mut stats = speed_reporter.speed_stats.lock().await;
                for (_file_id, stat) in stats.iter_mut() {
                    // 添加到历史数据
                    stat.bytes_history.push_back(stat.bytes_last_second);
                    if stat.bytes_history.len() > 5 {
                        // 保持5秒的历史
                        stat.bytes_history.pop_front();
                    }

                    // 重置本秒接收的字节数
                    stat.bytes_last_second = 0;
                    stat.last_updated = Instant::now();
                }
            }
        });

        // 定期检查是否有新的下载任务
        loop {
            // 获取所有待接收的文件，添加超时处理
            let files = match tokio::time::timeout(
                Duration::from_secs(5),
                self.service.get_pending_receive_files(),
            )
            .await
            {
                Ok(result) => match result {
                    Ok(files) => files,
                    Err(e) => {
                        error!("获取待接收文件失败: {}", e);
                        // 短暂延迟后继续
                        sleep(Duration::from_secs(1)).await;
                        continue;
                    }
                },
                Err(_) => {
                    warn!("获取待接收文件超时，将在1秒后重试");
                    sleep(Duration::from_secs(1)).await;
                    continue;
                }
            };

            let mut tasks = Vec::new();

            // 找出所有已接受但还未开始下载的文件
            for file in files.iter() {
                if file.status == TransferStatus::Accepted {
                    // 获取绝对保存路径并打印详细信息
                    let save_path = PathBuf::from(&file.file_absolute_path);

                    info!(
                        "准备下载文件: id={}, name={}, 保存路径={}",
                        file.file_id,
                        file.file_name,
                        save_path.display()
                    );

                    tasks.push(DownloadTask {
                        request_id: file.request_id.clone(),
                        file_id: file.file_id.clone(),
                        device_id: file.device_id.clone(),
                        save_path,
                        file_size: file.file_size,
                        is_dir: file.is_dir,
                        start_offset: file.progress,
                    });

                    // 更新文件状态为下载中
                    if let Some(mut file_entry) = self.service.received_files.get_mut(&file.file_id)
                    {
                        file_entry.status = TransferStatus::InProgress;
                    }
                }
            }

            // 为每个任务启动下载，限制同时启动的数量避免资源耗尽
            let max_concurrent_starts = 3;
            for chunk in tasks.chunks(max_concurrent_starts) {
                let mut futures = Vec::new();

                for task in chunk {
                    // 克隆必要的引用
                    let self_clone = Arc::clone(&self);
                    let semaphore = Arc::clone(&self.semaphore);
                    let task = task.clone();

                    // 将任务封装在一个有超时的异步块中
                    let future = tokio::spawn(async move {
                        // 获取信号量许可，添加超时
                        let permit = match tokio::time::timeout(
                            Duration::from_secs(10),
                            semaphore.acquire(),
                        )
                        .await
                        {
                            Ok(Ok(permit)) => permit,
                            Ok(Err(e)) => {
                                error!("获取信号量失败: {}", e);
                                return;
                            }
                            Err(_) => {
                                error!("获取信号量超时");
                                return;
                            }
                        };

                        info!(
                            "开始下载文件: id={}, path={}",
                            task.file_id,
                            task.save_path.display()
                        );

                        // 执行下载，添加超时保护
                        match tokio::time::timeout(
                            Duration::from_secs(3600), // 1小时超时
                            self_clone.start_download(task.clone()),
                        )
                        .await
                        {
                            Ok(Ok(_)) => {
                                info!("文件下载成功: {}", task.file_id);
                            }
                            Ok(Err(e)) => {
                                error!(
                                    "下载任务失败: request_id={}, file_id={}, error={}",
                                    task.request_id, task.file_id, e
                                );

                                // 更新文件状态为失败
                                if let Some(mut file_entry) =
                                    self_clone.service.received_files.get_mut(&task.file_id)
                                {
                                    file_entry.status = TransferStatus::Failed;
                                }
                            }
                            Err(_) => {
                                error!(
                                    "下载任务超时: request_id={}, file_id={}",
                                    task.request_id, task.file_id
                                );
                                // 更新文件状态为失败
                                if let Some(mut file_entry) =
                                    self_clone.service.received_files.get_mut(&task.file_id)
                                {
                                    file_entry.status = TransferStatus::Failed;
                                }
                            }
                        }

                        // 释放信号量许可
                        drop(permit);
                    });

                    futures.push(future);
                }

                // 等待当前批次的任务都启动完成
                for future in futures {
                    let _ = future.await;
                }
            }

            // 使用短暂延迟减轻CPU负担
            sleep(Duration::from_millis(500)).await;
        }
    }

    // 更新下载进度时同时更新received_files中的状态
    async fn update_progress(&self, progress: ProgressUpdate) -> Result<()> {
        // 构建并发送进度更新事件
        let transfer_progress = TransferProgress {
            request_id: progress.request_id.clone(),
            file_id: progress.file_id.clone(),
            bytes_received: progress.bytes_received,
            status: progress.status.clone(),
            error_message: progress.error_message.clone(),
            speed: progress.speed,
        };

        // 发送进度更新
        self.service
            .update_transfer_progress(transfer_progress)
            .await?;

        // 同时通过通道发送进度更新，用于内部处理
        if let Err(e) = self.progress_sender.send(progress.clone()).await {
            warn!("发送进度更新失败: {}", e);
        }

        // 然后再更新received_files中的文件进度和状态
        if let Some(mut file_entry) = self.service.received_files.get_mut(&progress.file_id) {
            file_entry.progress = progress.bytes_received;
            file_entry.status = progress.status;
        }

        Ok(())
    }

    // 向发送方报告进度的新方法
    async fn report_progress_to_sender(
        &self,
        request_id: &str,
        file_id: &str,
        bytes_received: u64,
        status: TransferStatus,
        error_message: Option<String>,
        speed: u64,
        device_info: &DeviceInfo,
    ) -> Result<()> {
        // 构建进度报告
        let progress = TransferProgress {
            request_id: request_id.to_string(),
            file_id: file_id.to_string(),
            bytes_received,
            status: status.clone(),
            error_message,
            speed,
        };

        info!(
            "正在向发送方 {}:{} 报告进度: 文件={}, 进度={}/{}, 状态={:?}",
            device_info.address[0],
            device_info.port,
            file_id,
            bytes_received,
            "未知总大小",
            progress.status
        );

        // 重试机制
        let max_retries = 3;
        let mut retry_count = 0;
        let mut last_error = None;

        while retry_count < max_retries {
            // 添加超时限制
            match tokio::time::timeout(
                Duration::from_secs(5),
                self.http_client.report_progress(
                    &progress,
                    &device_info.address[0],
                    device_info.port,
                ),
            )
            .await
            {
                Ok(Ok(_)) => {
                    debug!(
                        "成功向发送方报告进度: 文件={}, 状态={:?}",
                        file_id, progress.status
                    );
                    return Ok(());
                }
                Ok(Err(e)) => {
                    warn!(
                        "向发送方报告进度失败 (尝试 {}/{}): {}",
                        retry_count + 1,
                        max_retries,
                        e
                    );
                    last_error = Some(e);
                }
                Err(_) => {
                    warn!(
                        "向发送方报告进度超时 (尝试 {}/{})",
                        retry_count + 1,
                        max_retries
                    );
                    last_error = Some(HiveDropError::NetworkError(
                        "向发送方报告进度超时".to_string(),
                    ));
                }
            }

            retry_count += 1;

            if retry_count < max_retries {
                // 等待一段时间后重试
                tokio::time::sleep(Duration::from_millis(500 * retry_count as u64)).await;
            }
        }

        // 所有重试都失败
        Err(last_error.unwrap_or_else(|| {
            HiveDropError::NetworkError("向发送方报告进度失败，达到最大重试次数".to_string())
        }))
    }
}

// 启动下载管理器
pub async fn start(service: Arc<HttpTransferService>) -> Result<()> {
    info!("启动文件下载管理器...");

    // 创建下载管理器
    let (manager, mut progress_receiver) = DownloadManager::new(service.clone());
    let manager = Arc::new(manager);

    // 启动进度更新处理任务
    let progress_manager = Arc::clone(&manager);
    tokio::spawn(async move {
        while let Some(update) = progress_receiver.recv().await {
            debug!(
                "收到文件 {} 的进度更新: 状态={:?}, 已接收={} 字节",
                update.file_id, update.status, update.bytes_received
            );
        }
    });

    // 启动下载队列处理
    let download_manager = Arc::clone(&manager);
    tokio::spawn(async move {
        if let Err(e) = download_manager.process_download_queue().await {
            error!("下载队列处理出错: {}", e);
        }
    });

    info!("文件下载管理器已启动");
    Ok(())
}
