use std::{fs, path::Path};

use rtf_parser::{
    document::RtfDocument,
    header::RtfHeader,
    parser::{Painter, StyleBlock},
};

use crate::{
    error::Error, formats::rtf::decode::decode_rtf, model::{RawChapter, RawDocument, RawMetadata}, parser::{Loader, ParseOptions}, utils::text::escape_html,
};

pub struct RtfLoader;

impl Loader for RtfLoader {
    fn supports(&self, path: &Path) -> bool {
        path.extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("rtf"))
    }

    fn load(
        &self,
        path: &Path,
        _options: &ParseOptions,
    ) -> Result<RawDocument, Error> {
        let bytes = fs::read(path)?;

        let text = decode_rtf(&bytes);

        let document = RtfDocument::try_from(text)
            .map_err(|error| {
                Error::Parser(format!(
                    "failed to parse RTF document '{}': {}",
                    path.display(),
                    error,
                ))
            })?;

        let chapter = RawChapter {
            title: None,
            content: style_blocks_to_html(
                &document.body,
                &document.header,
            ),
        };

        Ok(RawDocument {
            metadata: RawMetadata {
                title: extract_title(path),
                author: None,
                description: None,
                language: None,
                cover_id: None,
            },
            chapters: vec![chapter],
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


/// Преобразует тело RTF в HTML.
fn style_blocks_to_html(
    body: &[StyleBlock],
    header: &RtfHeader,
) -> String {
    let mut html = String::new();
    let mut paragraph = String::new();

    for block in body {
        let text = block.text.as_str();

        if is_paragraph_break(text) {
            push_paragraph(
                &mut html,
                &mut paragraph,
            );

            continue;
        }

        paragraph.push_str(
            &format_text_fragment(
                text,
                &block.painter,
                header,
            ),
        );
    }

    push_paragraph(
        &mut html,
        &mut paragraph,
    );

    html
}

/// Проверяет, является ли RTF-блок разделителем абзаца.
fn is_paragraph_break(text: &str) -> bool {
    matches!(text, "\n" | "\r\n")
}

/// Добавляет накопленный текст как HTML-абзац.
fn push_paragraph(
    html: &mut String,
    paragraph: &mut String,
) {
    let content = paragraph.trim();

    if content.is_empty() {
        paragraph.clear();
        return;
    }

    html.push_str("<p>");
    html.push_str(content);
    html.push_str("</p>");

    paragraph.clear();
}

/// Преобразует форматированный RTF-фрагмент в HTML.
fn format_text_fragment(
    text: &str,
    painter: &Painter,
    header: &RtfHeader,
) -> String {
    let mut result = String::new();

    let escaped = escape_html(text)
        .replace("\r\n", "\n")
        .replace('\r', "")
        .replace('\n', "<br>");

    if painter.bold {
        result.push_str("<strong>");
    }

    if painter.italic {
        result.push_str("<em>");
    }

    if painter.underline {
        result.push_str("<u>");
    }

    if painter.strike {
        result.push_str("<s>");
    }

    if painter.superscript {
        result.push_str("<sup>");
    }

    if painter.subscript {
        result.push_str("<sub>");
    }

    let color = if painter.color_ref != 0 {
        header.color_table.get(&painter.color_ref)
    } else {
        None
    };

    if let Some(color) = color {
        result.push_str(&format!(
            "<span style=\"color:rgb({},{},{})\">",
            color.red,
            color.green,
            color.blue,
        ));
    }

    result.push_str(&escaped);

    if color.is_some() {
        result.push_str("</span>");
    }

    if painter.subscript {
        result.push_str("</sub>");
    }

    if painter.superscript {
        result.push_str("</sup>");
    }

    if painter.strike {
        result.push_str("</s>");
    }

    if painter.underline {
        result.push_str("</u>");
    }

    if painter.italic {
        result.push_str("</em>");
    }

    if painter.bold {
        result.push_str("</strong>");
    }

    result
}
