use sha2::{Digest, Sha256};

use std::path::Path;

pub fn mime_extension(mime: &str) -> &'static str {
    match mime {
        "image/jpeg" => "jpg",
        "image/png" => "png",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/svg+xml" => "svg",
        "image/bmp" => "bmp",
        "image/tiff" => "tiff",
        "image/x-icon" => "ico",
        _ => "",
    }
}

pub fn generate_resource_directory_name(path: &Path) -> String {
    let mut hasher = Sha256::new();

    hasher.update(path.to_string_lossy().as_bytes());

    let hash = hex::encode(hasher.finalize());

    hash[..16].to_owned()
}
