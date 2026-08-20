use std::{fs, io::Read, path::Path};

use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use encoding_rs::Encoding;
use roxmltree::{Document, Node};
use zip::ZipArchive;

use crate::{
    error::Error,
    model::{RawChapter, RawDocument, RawMetadata, RawResource},
    parser::{Loader, ParseOptions},
    utils::text::escape_html,
};

const XLINK_NS: &str = "http://www.w3.org/1999/xlink";

pub struct Fb2Loader;

impl Loader for Fb2Loader {
    fn supports(&self, path: &Path) -> bool {
        path.extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                matches!(extension.to_ascii_lowercase().as_str(), "fb2" | "zip")
            })
    }

    fn load(&self, path: &Path, _options: &ParseOptions) -> Result<RawDocument, Error> {
        let bytes = read_fb2_bytes(path)?;

        let xml = decode_xml(&bytes)?;

        let document = Document::parse(&xml).map_err(|error| {
            Error::Parser(format!(
                "failed to parse FB2 document '{}': {}",
                path.display(),
                error
            ))
        })?;

        let metadata = extract_metadata(&document);

        let resources = extract_resources(&document);

        let chapters = extract_chapters(&document);

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

fn extract_metadata(document: &Document<'_>) -> RawMetadata {
    RawMetadata {
        title: find_text(document, "book-title"),

        author: extract_author(document),

        description: extract_annotation(document),

        language: find_text(document, "lang"),

        cover_id: extract_cover_id(document),
    }
}

fn find_text(document: &Document<'_>, tag: &str) -> Option<String> {
    document
        .descendants()
        .find(|node| node.has_tag_name(tag))
        .and_then(|node| node.text())
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

fn extract_author(document: &Document<'_>) -> Option<String> {
    let author = document
        .descendants()
        .find(|node| node.has_tag_name("author"))?;

    let first_name = child_text(author, "first-name");

    let last_name = child_text(author, "last-name");

    let nickname = child_text(author, "nickname");

    let full_name = [first_name.as_deref(), last_name.as_deref()]
        .into_iter()
        .flatten()
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join(" ");

    if !full_name.is_empty() {
        Some(full_name)
    } else {
        nickname
    }
}

fn child_text(node: Node<'_, '_>, tag: &str) -> Option<String> {
    node.descendants()
        .find(|child| child.has_tag_name(tag))
        .and_then(|child| child.text())
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

fn extract_annotation(document: &Document<'_>) -> Option<String> {
    let annotation = document
        .descendants()
        .find(|node| node.has_tag_name("annotation"))?;

    let paragraphs = annotation
        .children()
        .filter(|node| node.has_tag_name("p"))
        .filter_map(|node| node.text())
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>();

    if paragraphs.is_empty() {
        None
    } else {
        Some(paragraphs.join("\n\n"))
    }
}

fn extract_cover_id(document: &Document<'_>) -> Option<String> {
    let image = document
        .descendants()
        .find(|node| node.has_tag_name("coverpage"))?
        .descendants()
        .find(|node| node.has_tag_name("image"))?;

    href_attr(image).map(|href| href.trim_start_matches('#').to_owned())
}

//
// MARK: - Resources
//

fn extract_resources(document: &Document<'_>) -> Vec<RawResource> {
    document
        .descendants()
        .filter(|node| node.has_tag_name("binary"))
        .filter_map(|node| {
            let id = node.attribute("id")?;

            let mime_type = node
                .attribute("content-type")
                .unwrap_or("application/octet-stream");

            let encoded = node
                .children()
                .filter_map(|child| child.text())
                .flat_map(str::chars)
                .filter(|character| !character.is_whitespace())
                .collect::<String>();

            if encoded.is_empty() {
                return None;
            }

            let data = BASE64.decode(encoded.as_bytes()).ok()?;

            Some(RawResource {
                id: id.to_owned(),
                mime_type: mime_type.to_owned(),
                data,
            })
        })
        .collect()
}

//
// MARK: - Chapters
//

fn extract_chapters(document: &Document<'_>) -> Vec<RawChapter> {
    let Some(body) = find_main_body(document) else {
        return Vec::new();
    };

    let mut chapters = Vec::new();

    collect_sections(body, &mut chapters);

    if chapters.is_empty() {
        let html = fb2_to_html(body);

        if !html.trim().is_empty() {
            chapters.push(RawChapter {
                title: None,
                content: html,
            });
        }
    }

    chapters
}

fn find_main_body<'a>(document: &'a Document<'a>) -> Option<Node<'a, 'a>> {
    document
        .descendants()
        .find(|node| node.has_tag_name("body") && node.attribute("name").is_none())
        .or_else(|| {
            document
                .descendants()
                .find(|node| node.has_tag_name("body"))
        })
}

fn collect_sections(node: Node<'_, '_>, chapters: &mut Vec<RawChapter>) {
    for section in node
        .children()
        .filter(|child| child.has_tag_name("section"))
    {
        let has_nested_sections = section
            .children()
            .any(|child| child.has_tag_name("section"));

        if has_nested_sections {
            collect_sections(section, chapters);
            continue;
        }

        let title = extract_section_title(section);

        let content = fb2_to_html(section);

        if content.trim().is_empty() {
            continue;
        }

        chapters.push(RawChapter { title, content });
    }
}

fn extract_section_title(section: Node<'_, '_>) -> Option<String> {
    let title = section.children().find(|node| node.has_tag_name("title"))?;

    let text = title
        .descendants()
        .filter_map(|node| node.text())
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join(" ");

    if text.is_empty() { None } else { Some(text) }
}

//
// MARK: - FB2 → HTML
//

fn fb2_to_html(node: Node<'_, '_>) -> String {
    if node.is_text() {
        return node.text().map(escape_html).unwrap_or_default();
    }

    if !node.is_element() {
        return String::new();
    }

    match node.tag_name().name() {
        "p" => wrap("p", render_children(node)),

        "title" => wrap("h2", render_children(node)),

        "subtitle" => wrap("h3", render_children(node)),

        "emphasis" => wrap("em", render_children(node)),

        "strong" => wrap("strong", render_children(node)),

        "strikethrough" => wrap("s", render_children(node)),

        "subscript" => wrap("sub", render_children(node)),

        "superscript" => wrap("sup", render_children(node)),

        "code" => wrap("code", render_children(node)),

        "empty-line" => "<br>".to_owned(),

        "image" => render_image(node),

        "epigraph" | "cite" => wrap("blockquote", render_children(node)),

        "poem" => wrap_class("div", "poem", render_children(node)),

        "stanza" => wrap_class("div", "stanza", render_children(node)),

        "v" => wrap_class("p", "verse", render_children(node)),

        "text-author" => wrap_class("p", "text-author", render_children(node)),

        "annotation" => wrap_class("div", "annotation", render_children(node)),

        "a" => render_link(node),

        "table" => wrap("table", render_children(node)),

        "tr" => wrap("tr", render_children(node)),

        "th" | "td" => wrap(node.tag_name().name(), render_children(node)),

        "section" | "body" => render_children(node),

        _ => render_children(node),
    }
}

fn render_children(node: Node<'_, '_>) -> String {
    node.children().map(fb2_to_html).collect()
}

fn render_image(node: Node<'_, '_>) -> String {
    let Some(href) = href_attr(node) else {
        return String::new();
    };

    let id = href.trim_start_matches('#');

    format!("<img src=\"{}\" alt=\"\" />", escape_html(id),)
}

fn render_link(node: Node<'_, '_>) -> String {
    let href = href_attr(node).unwrap_or("#");

    let content = render_children(node);

    format!("<a href=\"{}\">{}</a>", escape_html(href), content,)
}

fn wrap(tag: &str, content: String) -> String {
    format!("<{tag}>{content}</{tag}>")
}

fn wrap_class(tag: &str, class_name: &str, content: String) -> String {
    format!("<{tag} class=\"{class_name}\">{content}</{tag}>")
}

//
// MARK: - FB2 references
//

fn href_attr<'a, 'input>(node: Node<'a, 'input>) -> Option<&'a str> {
    node.attribute((XLINK_NS, "href"))
        .or_else(|| node.attribute("href"))
        .or_else(|| node.attribute("l:href"))
}

//
// MARK: - File reading
//

fn read_fb2_bytes(path: &Path) -> Result<Vec<u8>, Error> {
    if is_zip(path) {
        read_fb2_from_zip(path)
    } else {
        Ok(fs::read(path)?)
    }
}

fn is_zip(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("zip"))
}

fn read_fb2_from_zip(path: &Path) -> Result<Vec<u8>, Error> {
    let file = fs::File::open(path)?;

    let mut archive = ZipArchive::new(file).map_err(|error| {
        Error::Parser(format!(
            "failed to open ZIP archive '{}': {}",
            path.display(),
            error,
        ))
    })?;

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| Error::Parser(format!("failed to read ZIP entry: {}", error,)))?;

        let name = entry.name().to_ascii_lowercase();

        if !name.ends_with(".fb2") {
            continue;
        }

        let mut bytes = Vec::new();

        entry
            .read_to_end(&mut bytes)
            .map_err(|error| Error::Parser(format!("failed to read FB2 from ZIP: {}", error,)))?;

        return Ok(bytes);
    }

    Err(Error::InvalidDocument(
        "FB2 document was not found inside ZIP archive".to_owned(),
    ))
}

//
// MARK: - XML decoding
//

fn decode_xml(bytes: &[u8]) -> Result<String, Error> {
    let sniff = std::str::from_utf8(&bytes[..bytes.len().min(1024)]).unwrap_or("");

    let encoding = sniff.split("encoding").skip(1).find_map(|chunk| {
        let quote = chunk.find('"').or_else(|| chunk.find('\''))?;

        let rest = &chunk[quote + 1..];

        let end = rest.find('"').or_else(|| rest.find('\''))?;

        Some(rest[..end].trim().to_owned())
    });

    if let Some(encoding_name) = encoding {
        if let Some(encoding) = Encoding::for_label(encoding_name.as_bytes()) {
            let (text, _, had_errors) = encoding.decode(bytes);

            if had_errors {
                return Err(Error::InvalidDocument(format!(
                    "failed to decode FB2 using encoding '{}'",
                    encoding_name,
                )));
            }

            return Ok(text.into_owned());
        }
    }

    String::from_utf8(bytes.to_vec())
        .map_err(|error| Error::InvalidDocument(format!("failed to decode FB2 XML: {}", error,)))
}
