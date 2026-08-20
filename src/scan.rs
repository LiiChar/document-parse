use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub fn collect_paths(path: impl AsRef<Path>) -> Vec<PathBuf> {
    let extensions = [
        "txt", "epub", "fb2", "zip", "html", "htm", "md", "markdown", "docx", "pdf", "cbz", "mobi",
        "rtf",
    ];

    WalkDir::new(path)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .map(|e| e.into_path())
        .filter(|path| {
            path.extension()
                .and_then(|e| e.to_str())
                .map(|e| extensions.iter().any(|x| x.eq_ignore_ascii_case(e)))
                .unwrap_or(false)
        })
        .collect()
}
