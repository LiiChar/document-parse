use std::{collections::HashMap, fs::File, io::Read, path::Path};

use encoding_rs::Encoding;
use roxmltree::Document;
use zip::ZipArchive;

use crate::{
    error::Error,
    model::{RawChapter, RawDocument, RawMetadata, RawResource},
    parser::{Loader, ParseOptions},
};

pub struct EpubLoader;

impl Loader for EpubLoader {
    fn supports(&self, path: &Path) -> bool {
        path.extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("epub"))
    }

    fn load(&self, path: &Path, _options: &ParseOptions) -> Result<RawDocument, Error> {
        let file = File::open(path)?;

        let mut archive = ZipArchive::new(file).map_err(|error| {
            Error::Parser(format!(
                "failed to open EPUB '{}': {}",
                path.display(),
                error,
            ))
        })?;

        let container_xml = read_zip_file(&mut archive, "META-INF/container.xml")?;

        let opf_path = find_opf_path(&container_xml)?;

        let opf_xml = read_zip_file(&mut archive, &opf_path)?;

        let opf = parse_opf(&opf_xml)?;

        let base_dir = base_dir_from_path(&opf_path);

        let resources = extract_resources(&mut archive, &base_dir, &opf)?;

        let chapters = extract_chapters(&mut archive, &base_dir, &opf)?;

        let cover_id = resolve_cover_id(&opf);

        Ok(RawDocument {
            metadata: RawMetadata {
                title: opf.title,
                author: opf.author,
                description: opf.description,
                language: opf.language,
                cover_id,
            },
            chapters,
            resources,
        })
    }
}

//
// MARK: - OPF
//

#[derive(Debug, Default)]
struct EpubPackage {
    title: Option<String>,
    author: Option<String>,
    description: Option<String>,
    language: Option<String>,

    manifest: HashMap<String, ManifestItem>,
    spine: Vec<String>,

    cover_id: Option<String>,
}

#[derive(Debug, Clone)]
struct ManifestItem {
    href: String,
    media_type: String,
    properties: String,
}

fn parse_opf(xml: &str) -> Result<EpubPackage, Error> {
    let document = Document::parse(xml)
        .map_err(|error| Error::Parser(format!("failed to parse EPUB OPF: {}", error,)))?;

    let mut package = EpubPackage {
        title: find_metadata_text(&document, "title"),
        author: find_metadata_text(&document, "creator"),
        description: find_metadata_text(&document, "description"),
        language: find_metadata_text(&document, "language"),
        ..Default::default()
    };

    for item in document
        .descendants()
        .filter(|node| node.is_element() && node.tag_name().name() == "item")
    {
        let Some(id) = item.attribute("id") else {
            continue;
        };

        let Some(href) = item.attribute("href") else {
            continue;
        };

        package.manifest.insert(
            id.to_owned(),
            ManifestItem {
                href: href.to_owned(),
                media_type: item
                    .attribute("media-type")
                    .unwrap_or("application/octet-stream")
                    .to_owned(),
                properties: item.attribute("properties").unwrap_or("").to_owned(),
            },
        );
    }

    package.cover_id = document
        .descendants()
        .find(|node| {
            node.is_element()
                && node.tag_name().name() == "meta"
                && node.attribute("name") == Some("cover")
        })
        .and_then(|node| node.attribute("content"))
        .map(str::to_owned);

    for itemref in document
        .descendants()
        .filter(|node| node.is_element() && node.tag_name().name() == "itemref")
    {
        let Some(idref) = itemref.attribute("idref") else {
            continue;
        };

        if package.manifest.contains_key(idref) {
            package.spine.push(idref.to_owned());
        }
    }

    if package.spine.is_empty() {
        return Err(Error::InvalidDocument("EPUB spine is empty".to_owned()));
    }

    Ok(package)
}

fn find_metadata_text(document: &Document<'_>, tag: &str) -> Option<String> {
    document
        .descendants()
        .find(|node| node.is_element() && node.tag_name().name() == tag)
        .and_then(|node| node.text())
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

//
// MARK: - Chapters
//

fn extract_chapters(
    archive: &mut ZipArchive<File>,
    base_dir: &str,
    package: &EpubPackage,
) -> Result<Vec<RawChapter>, Error> {
    let mut chapters = Vec::with_capacity(package.spine.len());

    for idref in &package.spine {
        let Some(item) = package.manifest.get(idref) else {
            continue;
        };

        let zip_path = join_epub_path(base_dir, &item.href);

        let bytes = read_zip_bytes(archive, &zip_path)?;

        let html = decode_document(&bytes)?;

        let title = extract_html_title(&html);

        let html = normalize_epub_html(&html);

        chapters.push(RawChapter {
            title,
            content: html,
        });
    }

    Ok(chapters)
}

//
// MARK: - Resources
//

fn extract_resources(
    archive: &mut ZipArchive<File>,
    base_dir: &str,
    package: &EpubPackage,
) -> Result<Vec<RawResource>, Error> {
    let mut resources = Vec::new();

    for (id, item) in &package.manifest {
        if !item.media_type.starts_with("image/") {
            continue;
        }

        let zip_path = join_epub_path(base_dir, &item.href);

        let data = match read_zip_bytes(archive, &zip_path) {
            Ok(data) => data,
            Err(_) => continue,
        };

        if data.is_empty() {
            continue;
        }

        resources.push(RawResource {
            id: id.clone(),
            mime_type: item.media_type.clone(),
            data,
        });
    }

    Ok(resources)
}

//
// MARK: - EPUB HTML
//

fn normalize_epub_html(html: &str) -> String {
    // EPUB chapter всегда возвращаем как HTML.
    //
    // Здесь намеренно нет sanitize_html() —
    // это делает finalize().
    html.trim().to_owned()
}

fn extract_html_title(html: &str) -> Option<String> {
    let document = Document::parse(html).ok()?;

    if let Some(title) = document
        .descendants()
        .find(|node| node.is_element() && node.tag_name().name() == "title")
        .and_then(|node| node.text())
        .map(str::trim)
        .filter(|text| !text.is_empty())
    {
        return Some(title.to_owned());
    }

    if let Some(title) = document
        .descendants()
        .find(|node| node.is_element() && node.tag_name().name() == "h1")
        .and_then(|node| node.text())
        .map(str::trim)
        .filter(|text| !text.is_empty())
    {
        return Some(title.to_owned());
    }

    document
        .descendants()
        .find(|node| node.is_element() && node.tag_name().name() == "h2")
        .and_then(|node| node.text())
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

//
// MARK: - ZIP
//

fn read_zip_file(archive: &mut ZipArchive<File>, path: &str) -> Result<String, Error> {
    let bytes = read_zip_bytes(archive, path)?;

    decode_document(&bytes)
}

fn read_zip_bytes(archive: &mut ZipArchive<File>, path: &str) -> Result<Vec<u8>, Error> {
    let mut file = archive.by_name(path).map_err(|error| {
        Error::InvalidDocument(format!("EPUB resource '{}' not found: {}", path, error,))
    })?;

    let mut bytes = Vec::with_capacity(file.size() as usize);

    file.read_to_end(&mut bytes)?;

    Ok(bytes)
}

//
// MARK: - Paths
//

fn base_dir_from_path(path: &str) -> String {
    Path::new(path)
        .parent()
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .filter(|path| !path.is_empty())
        .unwrap_or_default()
}

fn join_epub_path(base_dir: &str, href: &str) -> String {
    let href = href.split('#').next().unwrap_or(href);

    if base_dir.is_empty() {
        normalize_zip_path(href)
    } else {
        normalize_zip_path(&format!("{}/{}", base_dir.trim_end_matches('/'), href,))
    }
}

/// Нормализует `.` и `..` в ZIP path.
fn normalize_zip_path(path: &str) -> String {
    let mut parts = Vec::new();

    for part in path.split('/') {
        match part {
            "" | "." => {}

            ".." => {
                parts.pop();
            }

            value => {
                parts.push(value);
            }
        }
    }

    parts.join("/")
}

//
// MARK: - Encoding
//

fn decode_document(bytes: &[u8]) -> Result<String, Error> {
    if let Ok(text) = std::str::from_utf8(bytes) {
        return Ok(text.to_owned());
    }

    let encoding = detect_xml_encoding(bytes);

    if let Some(encoding) = encoding {
        let (text, _, had_errors) = encoding.decode(bytes);

        if !had_errors {
            return Ok(text.into_owned());
        }
    }

    Ok(String::from_utf8_lossy(bytes).into_owned())
}

fn detect_xml_encoding(bytes: &[u8]) -> Option<&'static Encoding> {
    let prefix = String::from_utf8_lossy(&bytes[..bytes.len().min(1024)]);

    let lower = prefix.to_ascii_lowercase();

    let encoding_start = lower.find("encoding")?;

    let after = &prefix[encoding_start + "encoding".len()..];

    let quote = after.find('"').or_else(|| after.find('\''))?;

    let after_quote = &after[quote + 1..];

    let end = if after
        .get(quote..)
        .is_some_and(|value| value.starts_with('"'))
    {
        after_quote.find('"')
    } else {
        after_quote.find('\'')
    }?;

    let encoding_name = after_quote[..end].trim();

    Encoding::for_label(encoding_name.as_bytes())
}

fn find_opf_path(container_xml: &str) -> Result<String, Error> {
    let document = Document::parse(container_xml).map_err(|error| {
        Error::Parser(format!("failed to parse EPUB container.xml: {}", error,))
    })?;

    document
        .descendants()
        .find(|node| node.is_element() && node.tag_name().name() == "rootfile")
        .and_then(|node| node.attribute("full-path"))
        .filter(|path| !path.trim().is_empty())
        .map(str::to_owned)
        .ok_or_else(|| {
            Error::InvalidDocument("EPUB OPF path not found in container.xml".to_owned())
        })
}

fn resolve_cover_id(package: &EpubPackage) -> Option<String> {
    // EPUB 3:
    //
    // <item
    //     id="cover-image"
    //     href="images/cover.jpg"
    //     media-type="image/jpeg"
    //     properties="cover-image"
    // />
    if let Some((id, _)) = package.manifest.iter().find(|(_, item)| {
        item.properties
            .split_whitespace()
            .any(|property| property == "cover-image")
    }) {
        return Some(id.clone());
    }

    // EPUB 2:
    //
    // <meta name="cover" content="cover-image" />
    if let Some(id) = &package.cover_id {
        if package.manifest.contains_key(id) {
            return Some(id.clone());
        }
    }

    // Fallback:
    // ищем изображение с "cover" в имени.
    package
        .manifest
        .iter()
        .find(|(_, item)| {
            item.media_type.starts_with("image/")
                && Path::new(&item.href)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .map(|name| name.to_ascii_lowercase().contains("cover"))
                    .unwrap_or(false)
        })
        .map(|(id, _)| id.clone())
}
