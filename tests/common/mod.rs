use std::{fs, path::PathBuf};

use tempfile::{TempDir, tempdir};

pub struct TestFile {
    pub _dir: TempDir,
    pub path: PathBuf,
}

impl TestFile {
    pub fn new(name: &str, content: &str) -> Self {
        let dir = tempdir().expect("failed to create temp dir");

        let path = dir.path().join(name);

        fs::write(&path, content).expect("failed to write fixture");

        Self { _dir: dir, path }
    }
}
