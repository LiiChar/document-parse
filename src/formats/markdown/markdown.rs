use std::{fs, path::Path};

use pulldown_cmark::{
    html,
    Options,
    Parser,
};

use crate::{
    error::Error,
    model::{
        RawChapter,
        RawDocument,
        RawMetadata,
    },
    parser::{
        Loader,
        ParseOptions,
    },
    utils::text::decode_text,
};

pub struct MarkdownLoader;

impl Loader for MarkdownLoader {
    fn supports(&self, path: &Path) -> bool {
        path.extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                matches!(
                    extension.to_ascii_lowercase().as_str(),
                    "md" | "markdown"
                )
            })
    }

    fn load(
        &self,
        path: &Path,
        _options: &ParseOptions,
    ) -> Result<RawDocument, Error> {
        let bytes = fs::read(path)?;
        let markdown = decode_text(&bytes);

        let html = markdown_to_html(&markdown);

        Ok(RawDocument {
            metadata: RawMetadata {
                title: extract_title(path),
                author: None,
                description: None,
                language: None,
                cover_id: None,
            },
            chapters: vec![
                RawChapter {
                    title: None,
                    content: html,
                },
            ],
            resources: Vec::new(),
        })
    }
}

/// Извлекает название документа из имени файла.
fn extract_title(path: &Path) -> Option<String> {
    path.file_stem()
        .and_then(|value| value.to_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

/// Преобразует Markdown в HTML.
fn markdown_to_html(markdown: &str) -> String {
    let mut options = Options::empty();

    options.insert(
        Options::ENABLE_STRIKETHROUGH,
    );

    options.insert(
        Options::ENABLE_TABLES,
    );

    options.insert(
        Options::ENABLE_TASKLISTS,
    );

    options.insert(
        Options::ENABLE_FOOTNOTES,
    );

    options.insert(
        Options::ENABLE_HEADING_ATTRIBUTES,
    );

    let parser = Parser::new_ext(
        markdown,
        options,
    );

    let mut html_output = String::new();

    html::push_html(
        &mut html_output,
        parser,
    );

    html_output
}
