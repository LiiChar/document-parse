use std::{
    collections::HashSet,
    fs::File,
    io::{Read, Seek},
    path::Path,
};

use docx_rust::document::{
    BodyContent, Drawing, Paragraph, ParagraphContent, Run, RunContent, Table, TableCellContent,
    TableRowContent,
};
use docx_rust::formatting::NumberingProperty;
use docx_rust::{Docx, DocxFile};
use xmltree::Element;
use zip::ZipArchive;

use crate::{
    error::Error, model::{RawChapter, RawDocument, RawMetadata, RawResource}, parser::{Loader, ParseOptions}, utils::text::{escape_html, normalize_source_text},
};

pub struct DocxLoader;

impl Loader for DocxLoader {
    fn supports(&self, path: &Path) -> bool {
        path.extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("docx"))
    }

    fn load(&self, path: &Path, _options: &ParseOptions) -> Result<RawDocument, Error> {
        let docx_file = DocxFile::from_file(path).map_err(|error| {
            Error::Parser(format!(
                "failed to open DOCX '{}': {}",
                path.display(),
                error,
            ))
        })?;

        let docx = docx_file.parse().map_err(|error| {
            Error::Parser(format!(
                "failed to parse DOCX '{}': {}",
                path.display(),
                error,
            ))
        })?;

        let file = File::open(path)?;

        let mut archive = ZipArchive::new(file).map_err(|error| {
            Error::Parser(format!(
                "failed to open DOCX archive '{}': {}",
                path.display(),
                error,
            ))
        })?;

        let metadata = extract_metadata(&mut archive, path)?;

        let resources = extract_resources(&mut archive)?;

        let chapters = convert_document_to_chapters(&docx, &mut archive, &resources)?;

        Ok(RawDocument {
            metadata,
            chapters,
            resources,
        })
    }
}

//
// MARK: - Metadata
//

fn extract_metadata<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    path: &Path,
) -> Result<RawMetadata, Error> {
    let title_fallback = fallback_title(path);

    let metadata = match read_xml(archive, "docProps/core.xml") {
        Ok(document) => document,

        Err(_) => {
            return Ok(RawMetadata {
                title: Some(title_fallback),
                author: None,
                description: None,
                language: None,
                cover_id: None,
            });
        }
    };

    let title = metadata
        .get_child("title")
        .and_then(|element| element.get_text())
        .map(|text| text.trim().to_owned())
        .filter(|text| !text.is_empty())
        .or(Some(title_fallback));

    let author = metadata
        .get_child("creator")
        .and_then(|element| element.get_text())
        .map(|text| text.trim().to_owned())
        .filter(|text| !text.is_empty());

    let description = metadata
        .get_child("description")
        .and_then(|element| element.get_text())
        .map(|text| text.trim().to_owned())
        .filter(|text| !text.is_empty());

    Ok(RawMetadata {
        title,
        author,
        description,
        language: None,
        cover_id: None,
    })
}

fn fallback_title(path: &Path) -> String {
    path.file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("Untitled")
        .trim()
        .to_owned()
}

//
// MARK: - Resources
//

fn extract_resources<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
) -> Result<Vec<RawResource>, Error> {
    let mut resources = Vec::new();

    let file_names = archive
        .file_names()
        .map(str::to_owned)
        .collect::<HashSet<_>>();

    for file_name in file_names {
        if !file_name.starts_with("word/media/") {
            continue;
        }

        let mime_type = mime_from_path(&file_name);

        let mut file = archive.by_name(&file_name).map_err(|error| {
            Error::Parser(format!(
                "failed to read DOCX resource '{}': {}",
                file_name, error,
            ))
        })?;

        let mut data = Vec::new();

        file.read_to_end(&mut data)?;

        if data.is_empty() {
            continue;
        }

        let id = file_name
            .strip_prefix("word/media/")
            .unwrap_or(&file_name)
            .to_owned();

        resources.push(RawResource {
            id,
            mime_type: mime_type.to_owned(),
            data,
        });
    }

    Ok(resources)
}

//
// MARK: - Chapters
//

fn convert_document_to_chapters<R: Read + Seek>(
    docx: &Docx,
    archive: &mut ZipArchive<R>,
    resources: &[RawResource],
) -> Result<Vec<RawChapter>, Error> {
    let resource_ids = resources
        .iter()
        .map(|resource| resource.id.clone())
        .collect::<HashSet<_>>();

    let mut chapters = Vec::new();

    let mut current_html = String::with_capacity(32_000);

    let mut current_title = None;

    let mut in_list = false;
    let mut list_level = 0u8;
    let mut list_type = "ul";

    for body_content in &docx.document.body.content {
        match body_content {
            BodyContent::Paragraph(paragraph) => {
                let (paragraph_html, heading_level, list_info, plain_text) =
                    process_paragraph(paragraph, archive, docx, &resource_ids)?;

                let heading_level = heading_level.unwrap_or(0);

                let is_chapter = matches!(heading_level, 1 | 2) || is_chapter_title(&plain_text);

                if is_chapter {
                    close_list(&mut current_html, &mut in_list, list_type);

                    push_chapter(&mut chapters, &mut current_title, &mut current_html);

                    if !plain_text.is_empty() {
                        current_title = Some(plain_text);
                    }

                    continue;
                }

                if let Some((level, kind)) = list_info {
                    if !in_list || level != list_level || kind != list_type {
                        close_list(&mut current_html, &mut in_list, list_type);

                        list_type = if kind == "decimal" { "ol" } else { "ul" };

                        list_level = level;
                        in_list = true;

                        current_html.push_str(&format!("<{}>", list_type));
                    }

                    if !paragraph_html.trim().is_empty() {
                        current_html.push_str(&format!("<li>{}</li>", paragraph_html));
                    }

                    continue;
                }

                close_list(&mut current_html, &mut in_list, list_type);

                if paragraph_html.trim().is_empty() {
                    continue;
                }

                let tag = match heading_level {
                    3 => "h3",
                    4 => "h4",
                    5 => "h5",
                    _ => "p",
                };

                current_html.push_str(&format!("<{tag}>{}</{tag}>", paragraph_html));
            }

            BodyContent::Table(table) => {
                close_list(&mut current_html, &mut in_list, list_type);

                current_html.push_str(&process_table(table, archive, docx, &resource_ids)?);
            }

            _ => {}
        }
    }

    close_list(&mut current_html, &mut in_list, list_type);

    push_chapter(&mut chapters, &mut current_title, &mut current_html);

    if chapters.is_empty() {
        chapters.push(RawChapter {
            title: None,
            content: String::new(),
        });
    }

    Ok(chapters)
}

fn push_chapter(chapters: &mut Vec<RawChapter>, title: &mut Option<String>, html: &mut String) {
    if html.trim().is_empty() && title.is_none() {
        return;
    }

    chapters.push(RawChapter {
        title: title.take(),
        content: std::mem::take(html),
    });
}

fn close_list(html: &mut String, in_list: &mut bool, list_type: &str) {
    if !*in_list {
        return;
    }

    html.push_str(&format!("</{}>", list_type));

    *in_list = false;
}

//
// MARK: - Paragraphs
//

type ParagraphResult = (String, Option<u8>, Option<(u8, String)>, String);

fn process_paragraph<R: Read + Seek>(
    paragraph: &Paragraph,
    archive: &mut ZipArchive<R>,
    docx: &Docx,
    resource_ids: &HashSet<String>,
) -> Result<ParagraphResult, Error> {
    let mut html = String::new();

    let heading_level = paragraph
        .property
        .as_ref()
        .and_then(|property| property.style_id.as_ref())
        .and_then(|style| {
            let value = style.value.as_ref();

            value
                .strip_prefix("Heading")
                .and_then(|value| value.parse::<u8>().ok())
        });

    let list_info = paragraph
        .property
        .as_ref()
        .and_then(|property| property.numbering.as_ref())
        .map(|numbering| {
            let level = numbering
                .level
                .as_ref()
                .map(|level| {
                    if level.value < 0 {
                        0
                    } else {
                        level.value as u8
                    }
                })
                .unwrap_or(0);

            let kind = get_list_kind(docx, numbering);

            (level, kind)
        });

    for content in &paragraph.content {
        match content {
            ParagraphContent::Run(run) => {
                html.push_str(&process_run(run, archive, docx, resource_ids)?);
            }

            ParagraphContent::Link(link) => {
                html.push_str(&process_hyperlink(link, archive, docx, resource_ids)?);
            }

            _ => {}
        }
    }

    let plain_text = normalize_source_text(&paragraph.text());

    Ok((html, heading_level, list_info, plain_text))
}

//
// MARK: - Runs
//

fn process_run<R: Read + Seek>(
    run: &Run,
    archive: &mut ZipArchive<R>,
    docx: &Docx,
    resource_ids: &HashSet<String>,
) -> Result<String, Error> {
    let mut content = String::new();

    let (bold, italic, underline) = match run.property.as_ref() {
        Some(property) => (
            property.bold.is_some(),
            property.italics.is_some(),
            property.underline.is_some(),
        ),

        None => (false, false, false),
    };

    for child in &run.content {
        match child {
            RunContent::Text(text) => {
                content.push_str(&escape_html(&text.text));
            }

            RunContent::Break(_) => {
                content.push_str("<br>");
            }

            RunContent::Drawing(drawing) => {
                if let Some(image) = extract_image(drawing, archive, docx, resource_ids)? {
                    content.push_str(&image);
                }
            }

            _ => {}
        }
    }

    if content.is_empty() {
        return Ok(String::new());
    }

    if underline {
        content = format!("<u>{}</u>", content);
    }

    if italic {
        content = format!("<em>{}</em>", content);
    }

    if bold {
        content = format!("<strong>{}</strong>", content);
    }

    Ok(content)
}

//
// MARK: - Images
//

fn extract_image<R: Read + Seek>(
    drawing: &Drawing,
    _archive: &mut ZipArchive<R>,
    docx: &Docx,
    resource_ids: &HashSet<String>,
) -> Result<Option<String>, Error> {
    let graphic = drawing
        .inline
        .as_ref()
        .and_then(|inline| inline.graphic.as_ref())
        .or_else(|| {
            drawing
                .anchor
                .as_ref()
                .and_then(|anchor| anchor.graphic.as_ref())
        });

    let Some(graphic) = graphic else {
        return Ok(None);
    };

    let Some(picture) = graphic.data.children.first() else {
        return Ok(None);
    };

    let embed = picture.fill.blip.embed.as_ref();

    if embed.is_empty() {
        return Ok(None);
    }

    let Some(target) = docx
        .document_rels
        .as_ref()
        .and_then(|relationships| relationships.get_target(embed))
    else {
        return Ok(None);
    };

    let resource_id = normalize_resource_id(target, resource_ids);

    if !resource_ids.contains(&resource_id) {
        return Ok(None);
    }

    Ok(Some(format!(
        "<img src=\"{}\" alt=\"\">",
        escape_html(&resource_id,),
    )))
}

fn normalize_resource_id(target: &str, resource_ids: &HashSet<String>) -> String {
    if resource_ids.contains(target) {
        return target.to_owned();
    }

    target
        .strip_prefix("word/")
        .filter(|target| resource_ids.contains(*target))
        .unwrap_or(target)
        .to_owned()
}

//
// MARK: - Tables
//

fn process_table<R: Read + Seek>(
    table: &Table,
    archive: &mut ZipArchive<R>,
    docx: &Docx,
    resource_ids: &HashSet<String>,
) -> Result<String, Error> {
    let mut html = String::from("<table>");

    for row in &table.rows {
        html.push_str("<tr>");

        for cell in &row.cells {
            let TableRowContent::TableCell(cell) = cell else {
                continue;
            };

            for content in &cell.content {
                let TableCellContent::Paragraph(paragraph) = content;

                let (paragraph_html, _, _, _) =
                    process_paragraph(paragraph, archive, docx, resource_ids)?;

                html.push_str("<td>");

                html.push_str(&paragraph_html);

                html.push_str("</td>");
            }
        }

        html.push_str("</tr>");
    }

    html.push_str("</table>");

    Ok(html)
}

//
// MARK: - Hyperlinks
//

fn process_hyperlink<R: Read + Seek>(
    link: &docx_rust::document::Hyperlink,
    archive: &mut ZipArchive<R>,
    docx: &Docx,
    resource_ids: &HashSet<String>,
) -> Result<String, Error> {
    let mut inner = String::new();

    if let Some(run) = &link.content {
        inner.push_str(&process_run(run, archive, docx, resource_ids)?);
    } else {
        let text = normalize_source_text(&link.text());

        if !text.is_empty() {
            inner.push_str(&escape_html(&text));
        }
    }

    if inner.trim().is_empty() {
        return Ok(String::new());
    }

    let href = if let Some(id) = &link.id {
        docx.document_rels
            .as_ref()
            .and_then(|relationships| relationships.get_target(id.as_ref()))
            .map(str::to_owned)
    } else {
        link.anchor.as_ref().map(|anchor| format!("#{}", anchor))
    };

    match href {
        Some(href) => Ok(format!("<a href=\"{}\">{}</a>", escape_html(&href), inner,)),

        None => Ok(inner),
    }
}

//
// MARK: - Helpers
//

fn get_list_kind(docx: &Docx, numbering: &NumberingProperty) -> String {
    let Some(num_id) = numbering.id.as_ref() else {
        return "bullet".to_owned();
    };

    let Some(level) = numbering.level.as_ref() else {
        return "bullet".to_owned();
    };

    let Some(numbering) = docx.numbering.as_ref() else {
        return "bullet".to_owned();
    };

    let Some(details) = numbering.numbering_details(num_id.value) else {
        return "bullet".to_owned();
    };

    let format = details
        .levels
        .iter()
        .find(|level_details| level_details.i_level == Some(level.value))
        .and_then(|level_details| level_details.number_format.as_ref())
        .map(|format| format.value.as_ref());

    match format {
        Some("decimal") | Some("decimalZero") => "decimal".to_owned(),

        _ => "bullet".to_owned(),
    }
}

fn mime_from_path(path: &str) -> &'static str {
    let extension = Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase());

    match extension.as_deref() {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("bmp") => "image/bmp",
        Some("webp") => "image/webp",
        Some("svg") => "image/svg+xml",
        Some("tif" | "tiff") => "image/tiff",
        _ => "application/octet-stream",
    }
}

fn read_xml<R: Read + Seek>(archive: &mut ZipArchive<R>, path: &str) -> Result<Element, Error> {
    let mut file = archive
        .by_name(path)
        .map_err(|error| Error::Parser(format!("failed to read '{}': {}", path, error,)))?;

    let mut xml = String::new();

    file.read_to_string(&mut xml)?;

    Element::parse(xml.as_bytes())
        .map_err(|error| Error::Parser(format!("failed to parse '{}': {}", path, error,)))
}

fn is_chapter_title(text: &str) -> bool {
    let normalized = normalize_source_text(text);

    if normalized.is_empty() {
        return false;
    }

    // Лучше использовать существующую
    // реализацию из utils, если она уже
    // доступна в проекте.
    let words = normalized.split_whitespace().count();

    words <= 12
}
