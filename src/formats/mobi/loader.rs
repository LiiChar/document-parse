use std::fs;
use std::path::Path;

use mobi::Mobi;
use scraper::{ElementRef, Html, Selector};

use crate::{
    error::Error,
    model::{RawChapter, RawDocument, RawMetadata},
    parser::{Loader, ParseOptions},
};

pub struct MobiLoader;

impl Loader for MobiLoader {
    fn supports(&self, path: &Path) -> bool {
        path.extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                matches!(
                    extension.to_ascii_lowercase().as_str(),
                    "mobi" | "azw" | "azw3"
                )
            })
    }

    fn load(&self, path: &Path, _options: &ParseOptions) -> Result<RawDocument, Error> {
        let bytes = fs::read(path)?;

        let book = Mobi::new(&bytes).map_err(|error| {
            Error::Parser(format!(
                "failed to parse MOBI document '{}': {}",
                path.display(),
                error,
            ))
        })?;

        let chapters = match book.content_as_string() {
            Ok(html) => split_html_into_chapters(&html),

            Err(error) => {
                return Err(Error::Parser(format!(
                    "failed to extract MOBI content '{}': {}",
                    path.display(),
                    error,
                )));
            }
        };

        let title = normalize_optional(book.title());

        let author = book.author();

        let description = book.description();

        let language = normalize_mobi_language(&book);

        Ok(RawDocument {
            metadata: RawMetadata {
                title,
                author,
                description,
                language,
                cover_id: None,
            },
            chapters,
            resources: Vec::new(),
        })
    }
}

fn split_html_into_chapters(html: &str) -> Vec<RawChapter> {
    let document = Html::parse_document(html);

    let pagebreak_selector = match Selector::parse("mbp\\:pagebreak") {
        Ok(selector) => selector,

        Err(_) => {
            return fallback_chapter(html);
        }
    };

    let span_selector = match Selector::parse("span") {
        Ok(selector) => selector,

        Err(_) => {
            return fallback_chapter(html);
        }
    };

    let mut chapters = Vec::new();

    for (index, pagebreak) in document.select(&pagebreak_selector).enumerate() {
        let Some(span) = pagebreak.select(&span_selector).next() else {
            continue;
        };

        let content = span.html();

        if content.trim().is_empty() {
            continue;
        }

        let title = extract_chapter_title(span).or_else(|| Some(format!("Chapter {}", index + 1,)));

        chapters.push(RawChapter { title, content });
    }

    if chapters.is_empty() {
        return fallback_chapter(html);
    }

    chapters
}

fn fallback_chapter(html: &str) -> Vec<RawChapter> {
    if html.trim().is_empty() {
        return Vec::new();
    }

    vec![RawChapter {
        title: Some("Chapter 1".to_owned()),
        content: html.to_owned(),
    }]
}

fn extract_chapter_title(span: ElementRef<'_>) -> Option<String> {
    let bold_selector = Selector::parse("b").ok()?;

    let bold = span.select(&bold_selector).next()?;

    let parts = bold
        .text()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(|text| text.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect::<Vec<_>>();

    if parts.is_empty() {
        return None;
    }

    Some(match parts.as_slice() {
        [] => return None,

        [single] => single.clone(),

        [first, rest @ ..] => {
            format!("{} — {}", first, rest.join(" "),)
        }
    })
}

fn normalize_optional(value: String) -> Option<String> {
    let value = value.trim();

    if value.is_empty() {
        None
    } else {
        Some(value.to_owned())
    }
}

fn normalize_mobi_language(book: &Mobi) -> Option<String> {
    let language = format!("{:?}", book.language(),)
        .trim()
        .to_ascii_lowercase();

    if language.is_empty() || language == "unknown" || language == "none" {
        None
    } else {
        Some(language)
    }
}
