use log::{debug, info};
use mime_guess::from_path;
use std::fs;
use std::path::Path;
use walkdir::WalkDir;

use super::gen_id;
use crate::error::{HiveDropError, Result};
use crate::types::PendingSendFileInfo;

/// 将文件或目录转换为用于传输的 PendingSendFileInfo 结构体向量
///
/// # 参数
/// * `path` - 文件或目录的路径
/// * `excluded_patterns` - 可选的排除模式（如 "*.DS_Store", "Thumbs.db"）
///
/// # 返回
/// * `Result<Vec<PendingSendFileInfo>>` - PendingSendFileInfo结构体向量或错误
pub fn path_to_transfer_structure<P: AsRef<Path>>(
    path: P,
    excluded_patterns: Option<Vec<String>>,
) -> Result<Vec<PendingSendFileInfo>> {
    let mut result = Vec::new();
    let path = path.as_ref();

    info!("开始处理路径: {}", path.display());

    // 检查路径是否存在
    if !path.exists() {
        return Err(HiveDropError::InvalidPath(format!(
            "路径不存在: {}",
            path.display()
        )));
    }

    // 设置glob模式匹配器（如果提供了排除模式）
    let excludes = if let Some(patterns) = excluded_patterns {
        let mut matchers = Vec::new();
        for pattern in patterns {
            matchers.push(glob::Pattern::new(&pattern).map_err(|e| {
                HiveDropError::GenericError(format!("无效的glob模式 '{}': {}", pattern, e))
            })?);
            debug!("添加排除模式: {}", pattern);
        }
        Some(matchers)
    } else {
        None
    };

    // 检查路径是否应该被排除的辅助函数
    let should_exclude = |path: &Path| -> bool {
        if let Some(ref matchers) = excludes {
            if let Some(file_name) = path.file_name() {
                let file_name_str = file_name.to_string_lossy();
                for pattern in matchers {
                    if pattern.matches(&file_name_str) {
                        debug!("排除文件: {}", path.display());
                        return true;
                    }
                }
            }
        }
        false
    };

    // 如果是单个文件
    if path.is_file() {
        if !should_exclude(path) {
            let file_info = create_file_info(path, path.parent().unwrap_or(Path::new("")))?;
            debug!(
                "添加文件: {} (大小: {} 字节)",
                file_info.file_name, file_info.file_size
            );
            result.push(file_info);
        }
        info!("处理完成，共 {} 个文件", result.len());
        return Ok(result);
    }

    // 如果是目录，遍历它（添加文件和目录）
    if path.is_dir() {
        debug!("开始遍历目录: {}", path.display());

        // 使用父目录作为基础路径，这样相对路径会包含当前目录名
        let base_path = path.parent().unwrap_or(Path::new(""));

        // 添加当前目录作为第一个条目
        let root_dir_info = create_dir_info(path, base_path)?;
        debug!("添加根目录: {}", root_dir_info.file_name);
        result.push(root_dir_info);

        // 使用 WalkDir 遍历目录
        for entry in WalkDir::new(path)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let entry_path = entry.path();

            // 跳过根目录本身，因为已经添加过了
            if entry_path == path {
                continue;
            }

            // 跳过被排除的项目
            if should_exclude(entry_path) {
                continue;
            }

            if entry_path.is_dir() {
                // 处理目录
                let dir_info = create_dir_info(entry_path, base_path)?;
                debug!("添加目录: {}", dir_info.file_name);
                result.push(dir_info);
            } else {
                // 处理文件
                let file_info = create_file_info(entry_path, base_path)?;
                debug!(
                    "添加文件: {} (大小: {} 字节, 相对路径: {})",
                    file_info.file_name, file_info.file_size, file_info.file_relative_path
                );
                result.push(file_info);
            }
        }
    } else {
        return Err(HiveDropError::InvalidPath(format!(
            "路径不是文件或目录: {}",
            path.display()
        )));
    }

    info!("处理完成，共 {} 个项目", result.len());
    Ok(result)
}

/// 为目录创建 PendingSendFileInfo 结构体
fn create_dir_info(path: &Path, base_path: &Path) -> Result<PendingSendFileInfo> {
    let dir_name = path
        .file_name()
        .ok_or_else(|| HiveDropError::InvalidPath(format!("无法获取目录名: {}", path.display())))?
        .to_string_lossy()
        .to_string();

    // 计算相对路径，从base_path开始，并统一使用 / 作为分隔符
    let relative_path = path
        .strip_prefix(base_path)
        .unwrap_or(Path::new(&dir_name))
        .to_string_lossy()
        .replace('\\', "/");

    // 获取绝对路径
    let absolute_path = path
        .canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .to_string();

    Ok(PendingSendFileInfo {
        file_id: gen_id::generate_file_id(),
        file_name: dir_name,
        file_relative_path: relative_path,
        file_absolute_path: absolute_path,
        file_size: 0,
        mime_type: "inode/directory".to_string(), // 目录的MIME类型
        is_dir: true,
        progress: 0,
    })
}

/// 根据给定路径创建 PendingSendFileInfo 结构体
fn create_file_info(path: &Path, base_path: &Path) -> Result<PendingSendFileInfo> {
    let file_size = fs::metadata(path)
        .map_err(|e| HiveDropError::IoError(e))?
        .len();

    let file_name = path
        .file_name()
        .ok_or_else(|| HiveDropError::InvalidPath(format!("无法获取文件名: {}", path.display())))?
        .to_string_lossy()
        .to_string();

    // 计算相对路径，从base_path开始，并统一使用 / 作为分隔符
    let relative_path = path
        .strip_prefix(base_path)
        .unwrap_or(Path::new(&file_name))
        .to_string_lossy()
        .replace('\\', "/");

    // 获取绝对路径
    let absolute_path = path
        .canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .to_string();

    let mime_type = from_path(path).first_or_octet_stream().to_string();

    Ok(PendingSendFileInfo {
        file_id: gen_id::generate_file_id(),
        file_name,
        file_relative_path: relative_path,
        file_absolute_path: absolute_path,
        file_size,
        mime_type,
        is_dir: false,
        progress: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_single_file() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;
        let file_path = dir.path().join("test.txt");
        let mut file = File::create(&file_path)?;
        file.write_all(b"Hello, world!")?;

        let files = path_to_transfer_structure(&file_path, None)?;

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].file_name, "test.txt");
        assert_eq!(files[0].file_size, 13);
        assert_eq!(files[0].mime_type, "text/plain");
        assert!(!files[0].is_dir);

        Ok(())
    }

    #[test]
    fn test_directory() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;

        // 创建多个文件
        let file1_path = dir.path().join("test1.txt");
        let file2_path = dir.path().join("test2.txt");
        let sub_dir_path = dir.path().join("subdir");
        fs::create_dir(&sub_dir_path)?;
        let file3_path = sub_dir_path.join("test3.txt");

        File::create(&file1_path)?.write_all(b"File 1")?;
        File::create(&file2_path)?.write_all(b"File 2")?;
        File::create(&file3_path)?.write_all(b"File 3")?;

        // 创建一个空目录
        let empty_dir_path = dir.path().join("emptydir");
        fs::create_dir(&empty_dir_path)?;

        // 同时创建一个需要排除的文件
        let ds_store_path = dir.path().join(".DS_Store");
        File::create(&ds_store_path)?.write_all(b"DS Store data")?;

        let exclude_patterns = Some(vec![String::from("*.DS_Store")]);
        let files = path_to_transfer_structure(dir.path(), exclude_patterns)?;

        // 包括3个文件、2个目录（subdir, emptydir）和根目录
        assert!(files.len() >= 5);

        // 确保 .DS_Store 没有被包含
        assert!(!files.iter().any(|f| f.file_name == ".DS_Store"));

        // 验证相对路径
        let has_subdir_file = files
            .iter()
            .any(|f| f.file_relative_path.contains("subdir"));
        assert!(has_subdir_file);

        // 验证空目录
        let has_empty_dir = files.iter().any(|f| f.is_dir && f.file_name == "emptydir");
        assert!(has_empty_dir, "应该包含空目录");

        Ok(())
    }

    #[test]
    fn test_empty_directory() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;
        let empty_dir_path = dir.path().join("emptydir");
        fs::create_dir(&empty_dir_path)?;

        let files = path_to_transfer_structure(&empty_dir_path, None)?;

        // 空目录至少应该包含自己
        assert!(!files.is_empty(), "应该包含空目录本身");
        assert!(files.iter().any(|f| f.is_dir), "应该标记为目录");

        Ok(())
    }

    #[test]
    fn test_transfer_request() {
        let file_path = std::path::PathBuf::from("/home/chenzibo/git/hive-send/src");
        let files = path_to_transfer_structure(&file_path, None).unwrap();

        println!("{:?}", files);
    }
}
