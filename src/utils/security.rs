// file_path: src/utils/security.rs
use log::info;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;

use crate::error::Result;

/// 计算证书的SHA-256指纹
///
/// 读取证书文件并计算其SHA-256哈希值作为指纹
/// 返回十六进制格式的指纹字符串
pub fn calculate_cert_fingerprint(cert_data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(cert_data);
    let result = hasher.finalize();

    // 将哈希值转换为十六进制字符串，每两位之间用冒号分隔
    let fingerprint = result
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<Vec<String>>()
        .join(":");

    fingerprint
}

/// 获取现有证书的指纹
///
/// 如果证书文件存在，读取并计算其指纹
/// 如果证书不存在，返回None
pub fn get_cert_fingerprint() -> Option<String> {
    let cert_path = Path::new("certs/cert.pem");
    if cert_path.exists() {
        if let Ok(cert_data) = fs::read(cert_path) {
            Some(calculate_cert_fingerprint(&cert_data))
        } else {
            None
        }
    } else {
        None
    }
}

/// 自动生成自签名SSL证书
///
/// 当系统中不存在证书时，自动创建自签名证书用于HTTPS连接
/// 生成的证书会保存到certs目录下，以便下次启动时重用
/// 返回证书数据、私钥数据以及证书的SHA-256指纹
pub(crate) fn generate_self_signed_cert() -> Result<(Vec<u8>, Vec<u8>, String)> {
    info!("开始生成自签名证书...");

    // 创建证书目录
    let cert_dir = Path::new("certs");
    if !cert_dir.exists() {
        info!("创建证书存储目录: certs/");
        fs::create_dir_all(cert_dir)?;
    }

    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()])?;
    let cert_pem = cert.serialize_pem()?;
    let key_pem = cert.serialize_private_key_pem();

    // 将证书写入文件以备后用
    fs::write("certs/cert.pem", &cert_pem)?;
    info!("证书已保存至: certs/cert.pem");

    fs::write("certs/key.pem", &key_pem)?;
    info!("私钥已保存至: certs/key.pem");

    // 计算证书指纹
    let fingerprint = calculate_cert_fingerprint(cert_pem.as_bytes());
    info!("证书指纹: {}", fingerprint);

    Ok((cert_pem.into_bytes(), key_pem.into_bytes(), fingerprint))
}
