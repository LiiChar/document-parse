use std::{fs, path::Path};

use crate::{
    error::Error,
    model::{RawChapter, RawDocument, RawMetadata},
    parser::{Loader, ParseOptions},
    utils::text::decode_text,
};

pub struct HtmlLoader;

impl Loader for HtmlLoader {
    fn supports(&self, path: &Path) -> bool {
        if path.starts_with("content://") {
            return true;
        }

        path.extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                matches!(extension.to_ascii_lowercase().as_str(), "html" | "htm")
            })
    }

    fn load(&self, path: &Path, _options: &ParseOptions) -> Result<RawDocument, Error> {
        let bytes = fs::read(path)?;
        let html = decode_text(&bytes);

        Ok(RawDocument {
            metadata: RawMetadata {
                title: extract_title(path, &html),
                author: None,
                description: None,
                language: None,
                cover_id: None,
            },
            chapters: vec![RawChapter {
                title: None,
                content: html,
            }],
            resources: Vec::new(),
        })
    }
}

/// Извлекает название HTML-документа.
///
/// Сначала пытается получить `<title>`.
/// Если `<title>` отсутствует, используется имя файла.
fn extract_title(path: &Path, html: &str) -> Option<String> {
    extract_html_title(html).or_else(|| {
        path.file_stem()
            .and_then(|value| value.to_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    })
}

/// Извлекает содержимое `<title>`.
///
/// Это намеренно простая реализация: HTML loader не должен
/// превращаться в полноценный HTML parser только ради metadata.
fn extract_html_title(html: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();

    let start = lower.find("<title")?;
    let start_tag_end = lower[start..].find('>')? + start + 1;

    let end = lower[start_tag_end..].find("</title>")? + start_tag_end;

    let title = &html[start_tag_end..end];

    let title = strip_html_tags(title).trim().to_owned();

    if title.is_empty() { None } else { Some(title) }
}

/// Удаляет HTML-теги из небольшого metadata-фрагмента.
fn strip_html_tags(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let mut inside_tag = false;

    for character in value.chars() {
        match character {
            '<' => {
                inside_tag = true;
            }

            '>' => {
                inside_tag = false;
            }

            _ if !inside_tag => {
                result.push(character);
            }

            _ => {}
        }
    }

    result
}
