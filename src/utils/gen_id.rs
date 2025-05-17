// file_path: src/utils/gen_id.rs
use rand::Rng;
use uuid::Uuid;

/// 生成唯一ID
#[allow(dead_code)]
pub fn generate_unique_id() -> String {
    Uuid::new_v4().to_string()
}

/// 生成文件ID
#[allow(dead_code)]
pub fn generate_file_id() -> String {
    Uuid::new_v4().to_string()
}

/// 生成指定长度的随机字符串
pub fn generate_random_string(length: usize) -> String {
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::rng();

    (0..length)
        .map(|_| {
            let idx = rng.random_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}
