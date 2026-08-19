use std::{
    fs,
    path::Path,
};

use pdf_extract::Document as PdfDocument;

use crate::{
    error::Error,
    model::{
        RawChapter,
        RawDocument,
        RawMetadata,
    },
    parser::{Loader, ParseOptions},
    utils::text::escape_html,
};

pub struct PdfLoader;

impl Loader for PdfLoader {
    fn supports(&self, path: &Path) -> bool {
        path.extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                extension.eq_ignore_ascii_case("pdf")
            })
    }

    fn load(
        &self,
        path: &Path,
        _options: &ParseOptions,
    ) -> Result<RawDocument, Error> {
        let bytes = fs::read(path)?;

        let document =
            PdfDocument::load_mem(&bytes)
                .map_err(|error| {
                    Error::Parser(format!(
                        "failed to parse PDF '{}': {}",
                        path.display(),
                        error,
                    ))
                })?;

        let chapters =
            extract_chapters(
                &document,
            )?;

        let title =
            path.file_stem()
                .and_then(|value| value.to_str())
                .filter(|value| !value.trim().is_empty())
                .map(str::to_owned);

        Ok(RawDocument {
            metadata: RawMetadata {
                title,
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

fn extract_chapters(
    document: &PdfDocument,
) -> Result<Vec<RawChapter>, Error> {
    let pages =
        document.get_pages();

    let mut chapters =
        Vec::with_capacity(
            pages.len(),
        );

    for (index, page_number) in
        pages.keys().enumerate()
    {
        let text =
            document
                .extract_text(&[*page_number])
                .map_err(|error| {
                    Error::Parser(format!(
                        "failed to extract PDF page {}: {}",
                        page_number,
                        error,
                    ))
                })?;

        let text =
            normalize_pdf_text(
                &text,
            );

        if text.trim().is_empty() {
            continue;
        }

        chapters.push(
            RawChapter {
                title: Some(format!(
                    "Page {}",
                    index + 1,
                )),
                content: text_to_html(
                    &text,
                ),
            },
        );
    }

    Ok(chapters)
}

fn normalize_pdf_text(
    text: &str,
) -> String {
    let text =
        text.replace("\r\n", "\n")
            .replace('\r', "\n");

    let mut result =
        String::new();

    let mut previous_empty =
        false;

    for line in text.lines() {
        let line =
            line.trim();

        if line.is_empty() {
            if !previous_empty {
                result.push('\n');
            }

            previous_empty = true;
            continue;
        }

        previous_empty = false;

        if result.ends_with('-') {
            result.pop();
            result.push_str(line);
        } else {
            if !result.is_empty()
                && !result.ends_with('\n')
            {
                result.push(' ');
            }

            result.push_str(line);
        }
    }

    result
        .trim()
        .to_owned()
}

fn text_to_html(
    text: &str,
) -> String {
    let mut html =
        String::new();

    for paragraph in
        text.split("\n\n")
    {
        let paragraph =
            paragraph.trim();

        if paragraph.is_empty() {
            continue;
        }

        html.push_str("<p>");
        html.push_str(
            &escape_html(
                paragraph,
            ),
        );
        html.push_str("</p>");
    }

    html
}
