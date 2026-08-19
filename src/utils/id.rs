use std::path::Path;

pub fn generate_id(path: &Path) -> String {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    hasher.update(path.to_string_lossy().as_bytes());

    hex::encode(hasher.finalize())
}
