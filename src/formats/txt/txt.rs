use std::{fs, path::Path};

use crate::{
    error::Error, model::{RawChapter, RawDocument, RawMetadata}, parser::{Loader, ParseOptions}, utils::text::{decode_text, split_into_chapters, text_to_html},
};

pub struct TxtLoader;

impl Loader for TxtLoader {
    fn supports(&self, path: &Path) -> bool {
        path.extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.eq_ignore_ascii_case("txt"))
            .unwrap_or(false)
    }

    fn load(
        &self,
        path: &Path,
        options: &ParseOptions,
    ) -> Result<RawDocument, Error> {
        let bytes = fs::read(path)?;
        let text = decode_text(&bytes);

        let title = extract_title(path);

        let chapters = if options.split_txt_chapters {
            split_into_chapters(&text)
                .into_iter()
                .map(|chapter| RawChapter {
                    title: chapter.title,
                    content: text_to_html(&chapter.content.content()),
                })
                .collect()
        } else {
            vec![RawChapter {
                title: None,
                content: text_to_html(&text),
            }]
        };

        Ok(RawDocument {
            metadata: RawMetadata {
                title: Some(title),
                author: None,
                description: None,
                language: None,
                cover_id: None,
            },
            chapters,
            resources: Vec::new(),
        })
    }
}

/// Извлекает название документа из имени файла.
fn extract_title(path: &Path) -> String {
    path.file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("Untitled")
        .trim()
        .to_owned()
}
