mod file_handler;
mod status_handler;
mod transfer_handler;

use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;

use axum::Router;
use axum_server::tls_rustls::RustlsConfig;
use log::info;
use tower_http::trace::TraceLayer;

use crate::error::{HiveDropError, Result};
use crate::http::HttpTransferService;
use crate::utils::security;

/// 初始化所有路由
///
/// 该函数负责注册所有HTTP处理器，包括状态查询、文件传输、
/// 传输请求和传输控制等核心功能的处理器
fn init(service: Arc<HttpTransferService>) -> Router {
    // 合并所有路由，确保使用正确的状态类型
    let router = Router::new();
    let router = router.merge(status_handler::register());
    let router = router.merge(file_handler::register());
    let router = router.merge(transfer_handler::register());

    // 添加全局中间件
    router.with_state(service).layer(TraceLayer::new_for_http())
}

/// 启动HTTPS传输服务器
///
/// 该函数仅启动HTTPS服务器，使用指定的地址和服务配置中的端口
/// 如果HTTPS配置失败，则返回错误
pub async fn start(service: Arc<HttpTransferService>) -> Result<()> {
    info!("准备启动HTTPS传输服务...");

    // 从服务配置中获取端口
    let port = service.get_server_port();
    let addr = SocketAddr::from(([0, 0, 0, 0], port));

    info!("HTTPS服务将监听地址: {}", addr);
    info!("HTTPS服务将监听端口: {}", port);

    // 确保证书存在
    let cert_path = Path::new("certs/cert.pem");
    let key_path = Path::new("certs/key.pem");

    let fingerprint: String;
    if !cert_path.exists() || !key_path.exists() {
        info!("未找到现有证书，开始生成新的自签名证书...");
        let (_, _, fp) = security::generate_self_signed_cert()?;
        fingerprint = fp;
    } else {
        // 获取现有证书的指纹
        fingerprint = security::get_cert_fingerprint().unwrap_or_else(|| {
            info!("无法读取现有证书指纹，重新生成证书...");
            let (_, _, fp) = security::generate_self_signed_cert().unwrap();
            fp
        });
    }

    info!("证书指纹: {}", fingerprint);

    // 从文件加载TLS配置
    let rustls_config = match RustlsConfig::from_pem_file(cert_path, key_path).await {
        Ok(config) => {
            info!("TLS配置加载成功");
            config
        }
        Err(e) => {
            log::error!("TLS配置加载失败: {:?}", e);
            return Err(HiveDropError::CryptoError(format!(
                "TLS配置加载失败: {:?}",
                e
            )));
        }
    };

    // 构建应用，传入服务实例
    let app = init(service);

    // 启动HTTPS服务器
    info!("启动HTTPS服务器");
    axum_server::tls_rustls::bind_rustls(addr, rustls_config)
        .serve(app.into_make_service_with_connect_info::<SocketAddr>())
        .await?;

    Ok(())
}
