// file_path: src/utils/file_entropy.rs
use rand::Rng;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

pub fn calculate_random_blocks_entropy<P: AsRef<Path>>(
    filepath: P,
    block_size: usize,
    num_blocks: usize,
) -> io::Result<Vec<f64>> {
    let mut file = File::open(filepath)?;
    let file_size = file.metadata()?.len() as usize;

    if file_size < block_size {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "文件大小（{}字节）小于块大小（{}字节）",
                file_size, block_size
            ),
        ));
    }

    let max_start_pos = file_size - block_size;
    let mut rng = rand::rng();
    let mut entropies = Vec::with_capacity(num_blocks);
    let mut buffer = vec![0u8; block_size];

    for _ in 0..num_blocks {
        let start_pos = rng.random_range(0..=max_start_pos as u64);
        file.seek(SeekFrom::Start(start_pos))?;
        file.read_exact(&mut buffer)?;
        entropies.push(calculate_entropy(&buffer));
    }

    Ok(entropies)
}

fn calculate_entropy(data: &[u8]) -> f64 {
    let mut counts = [0u64; 256];
    let len = data.len() as f64;

    for &byte in data {
        counts[byte as usize] += 1;
    }

    counts
        .iter()
        .filter(|&&c| c > 0)
        .map(|&c| {
            let p = c as f64 / len;
            -p * p.log2()
        })
        .sum()
}

pub fn sample_file_entropy<P: AsRef<Path>>(
    filepath: P,
    block_size: usize,
    num_blocks: usize,
) -> Result<f64, String> {
    calculate_random_blocks_entropy(filepath, block_size, num_blocks)
        .map(|entropies| entropies.iter().sum::<f64>() / entropies.len() as f64)
        .map_err(|e| format!("计算文件熵时出错: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_entropy() {
        let data = b"hello world";
        let entropy = calculate_entropy(data);
        // 这里只需断言结果是否在合理范围内
        assert!(entropy > 2.8 && entropy < 2.9);
    }

    #[test]
    fn test_calculate_random_blocks_entropy() {
        let entropies = sample_file_entropy("target/debug/hive_drop", 128, 32).unwrap();
        println!("平均熵值: {}", entropies);
    }
}
