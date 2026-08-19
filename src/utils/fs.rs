
use crate::error::Error;

use sha2::{Sha256, Digest};

use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::model::Chapter;
use crate::utils::html::count_text_chars;


#[derive(Debug, Clone)]
pub struct FileInfo {
    pub size: u64,
    pub created_at: Option<u64>,
    pub modified_at: Option<u64>,
}

pub fn file_info(path: impl AsRef<Path>) -> Result<FileInfo, Error> {
    let metadata = fs::metadata(path)?;

    Ok(FileInfo {
        size: metadata.len(),
        created_at: system_time_to_unix(metadata.created().ok()),
        modified_at: system_time_to_unix(metadata.modified().ok()),
    })
}

fn system_time_to_unix(time: Option<SystemTime>) -> Option<u64> {
    time.and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
}



pub fn total_chars(chapters: &[Chapter]) -> u64 {
    chapters
        .iter()
        .map(|c| count_text_chars(&c.content.content()))
        .sum()
}

use encoding_rs::{UTF_16BE, UTF_16LE, UTF_8};

/// Пытается декодировать текст из популярных кодировок.
pub fn decode_text(bytes: &[u8]) -> Result<String, Error> {
    // UTF-8
    let (text, _, had_errors) = UTF_8.decode(bytes);
    if !had_errors {
        return Ok(text.into_owned());
    }

    // UTF-16 LE
    let (text, _, had_errors) = UTF_16LE.decode(bytes);
    if !had_errors {
        return Ok(text.into_owned());
    }

    // UTF-16 BE
    let (text, _, had_errors) = UTF_16BE.decode(bytes);
    if !had_errors {
        return Ok(text.into_owned());
    }

    Err(Error::Parser(
        "failed to decode text: unsupported encoding".into(),
    ))
}

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


pub fn generate_resource_directory_name(
    path: &Path,
) -> String {
    let mut hasher = Sha256::new();

    hasher.update(
        path.to_string_lossy().as_bytes()
    );

    let hash = hex::encode(hasher.finalize());

    hash[..16].to_owned()
}
