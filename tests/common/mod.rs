use std::{
    fs,
    path::{Path, PathBuf},
};

use document_parser::{DocumentParser, ParseOptions, model::{Document, RawDocument}};
use tempfile::{tempdir, TempDir};


pub struct TestFile {
    pub _dir: TempDir,
    pub path: PathBuf,
}

impl TestFile {
    pub fn new(
        name: &str,
        content:  &str,
    ) -> Self {
        let dir = tempdir()
            .expect("failed to create temp dir");

        let path = dir.path().join(name);

        fs::write(&path, content)
            .expect("failed to write fixture");

        Self {
            _dir: dir,
            path,
        }
    }
}

pub fn assert_raw_document_html(
    raw: &RawDocument,
) {
    for chapter in &raw.chapters {
        assert!(
            chapter.content.trim().is_empty()
                || chapter.content.contains('<'),
            "RawChapter content must contain HTML"
        );
    }
}

pub fn parse_with_options(
    path: &Path,
    options: ParseOptions,
) -> Document {
    DocumentParser::new()
        .with_options(options)
        .parse(path)
        .expect("document parsing failed")
}
