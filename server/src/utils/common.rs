use sha2::{Digest, Sha256};

/// 对数据执行两次 sha256（sha256d）
pub fn sha256d(data: &[u8]) -> Vec<u8> {
    let first = Sha256::digest(data);
    let second = Sha256::digest(&first);
    second.to_vec()
}
