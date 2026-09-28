use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use sha2::{Digest, Sha256};

use crate::{
    error::Error,
    model::{Chapter, ChapterContent, Content, Document, Metadata, RawDocument, RawResource},
    utils::{
        fs::{generate_resource_directory_name, mime_extension},
        html::{html_to_text, normalize_html_whitespace, sanitize_html},
        id::generate_id,
        language::detect_document_language,
        text::fallback_title,
    },
};

#[cfg(feature = "cbz")]
use crate::formats::cbz::CbzLoader;
#[cfg(feature = "djvu")]
use crate::formats::djvu::loader::DjvuLoader;
#[cfg(feature = "docx")]
use crate::formats::docx::DocxLoader;
#[cfg(feature = "epub")]
use crate::formats::epub::EpubLoader;
#[cfg(feature = "fb2")]
use crate::formats::fb2::Fb2Loader;
#[cfg(feature = "html")]
use crate::formats::html::HtmlLoader;
#[cfg(feature = "markdown")]
use crate::formats::markdown::MarkdownLoader;
#[cfg(feature = "mobi")]
use crate::formats::mobi::MobiLoader;
#[cfg(feature = "pdf")]
use crate::formats::pdf::PdfLoader;
#[cfg(feature = "rtf")]
use crate::formats::rtf::RtfLoader;
#[cfg(feature = "txt")]
use crate::formats::txt::TxtLoader;

pub trait Loader: Send + Sync {
    fn supports(&self, path: &Path) -> bool;

    fn load(&self, path: &Path, options: &ParseOptions) -> Result<RawDocument, Error>;
}

#[derive(Debug, Clone)]
pub enum ImageLoadType {
    Base64,
    Paths,
    None,
}

#[derive(Debug, Clone)]
pub enum ContentType {
    Html,
    Text,
}

#[derive(Debug, Clone)]
pub struct ParseOptions {
    pub content_type: ContentType,
    pub image_load: ImageLoadType,
    pub detect_language: bool,

    pub sanitize_html: bool,
    pub split_txt_chapters: bool,
    pub max_language_chars: usize,
    pub normalize_whitespace: bool,
}

impl Default for ParseOptions {
    fn default() -> Self {
        Self {
            content_type: ContentType::Html,
            image_load: ImageLoadType::Base64,
            detect_language: true,
            sanitize_html: true,
            split_txt_chapters: true,
            max_language_chars: 10_000,
            normalize_whitespace: true,
        }
    }
}

pub struct DocumentParser {
    loaders: Vec<Box<dyn Loader>>,
    options: ParseOptions,
}

impl Default for DocumentParser {
    fn default() -> Self {
        Self::new()
    }
}

impl DocumentParser {
    pub fn new() -> Self {
        let loaders: Vec<Box<dyn Loader>> = vec![
            #[cfg(feature = "txt")]
            Box::new(TxtLoader),
            #[cfg(feature = "markdown")]
            Box::new(MarkdownLoader),
            #[cfg(feature = "html")]
            Box::new(HtmlLoader),
            #[cfg(feature = "rtf")]
            Box::new(RtfLoader),
            #[cfg(feature = "fb2")]
            Box::new(Fb2Loader),
            #[cfg(feature = "epub")]
            Box::new(EpubLoader),
            #[cfg(feature = "cbz")]
            Box::new(CbzLoader),
            #[cfg(feature = "docx")]
            Box::new(DocxLoader),
            #[cfg(feature = "mobi")]
            Box::new(MobiLoader),
            #[cfg(feature = "pdf")]
            Box::new(PdfLoader),
            #[cfg(feature = "djvu")]
            Box::new(DjvuLoader),
        ];

        Self {
            loaders,
            options: ParseOptions::default(),
        }
    }

    pub fn with_options(mut self, options: ParseOptions) -> Self {
        self.options = options;
        self
    }

    pub fn register_loader(mut self, loader: impl Loader + 'static) -> Self {
        self.loaders.push(Box::new(loader));
        self
    }

    pub fn parse(&self, path: impl AsRef<Path>) -> Result<Document, Error> {
        let path = path.as_ref();

        for loader in &self.loaders {
            if loader.supports(path) {
                let raw = loader.load(path, &self.options)?;
                return self.finalize(raw, path);
            }
        }

        Err(Error::UnsupportedFormat)
    }

    pub fn finalize(&self, raw: RawDocument, path: &Path) -> Result<Document, Error> {
        let RawDocument {
            metadata,
            chapters: raw_chapters,
            resources,
        } = raw;

        // Сначала извлекаем cover, пока RawResource
        // ещё содержит исходные байты.
        let cover = metadata
            .cover_id
            .as_deref()
            .and_then(|cover_id| find_resource(&resources, cover_id))
            .map(|resource| resource.data.clone());

        // Обрабатываем ресурсы, на которые ссылается HTML.
        let resources = self.process_resources(resources, path)?;

        let chapters = raw_chapters
            .into_iter()
            .map(|chapter| self.finalize_chapter(chapter.title, chapter.content, &resources))
            .collect::<Vec<_>>();

        let language = if self.options.detect_language {
            detect_document_language_with_limit(&chapters, self.options.max_language_chars)
        } else {
            metadata.language
        };

        Ok(Document {
            metadata: Metadata {
                id: generate_id(path),
                title: fallback_title(path, metadata.title),
                author: metadata.author,
                description: metadata.description,
                language,
                cover,
            },
            content: Content { chapters },
        })
    }

    fn finalize_chapter(
        &self,
        title: Option<String>,
        html: String,
        resources: &ProcessedResources,
    ) -> Chapter {
        let content = match self.options.content_type {
            ContentType::Html => {
                // Sanitize untrusted source markup before inserting generated resource URLs.
                // In particular, ammonia correctly rejects `data:` URLs from source HTML,
                // while Base64 URLs produced from the document's own resources are trusted.
                let html = if self.options.sanitize_html {
                    sanitize_html(&html)
                } else {
                    html
                };

                let html = replace_resource_references(&html, resources);

                let html = if self.options.normalize_whitespace {
                    normalize_html_whitespace(&html)
                } else {
                    html
                };

                ChapterContent::Html(html)
            }

            ContentType::Text => {
                let text = html_to_text(&html);

                // let text = if self.options.normalize_whitespace {
                //     normalize_whitespace(&text)
                // } else {
                //     text
                // };

                ChapterContent::Text(text)
            }
        };

        Chapter { title, content }
    }

    fn process_resources(
        &self,
        resources: Vec<RawResource>,
        document_path: &Path,
    ) -> Result<ProcessedResources, Error> {
        let mut processed = ProcessedResources::default();

        for resource in resources {
            let id = resource.id.clone();

            let processed_resource = match self.options.image_load {
                ImageLoadType::None => ProcessedResource::None,

                ImageLoadType::Base64 => {
                    let data = BASE64.encode(&resource.data);

                    let source = format!("data:{};base64,{}", resource.mime_type, data,);

                    ProcessedResource::Source(source)
                }

                ImageLoadType::Paths => {
                    let path = self.save_resource(document_path, &resource)?;

                    ProcessedResource::Source(path.to_string_lossy().into_owned())
                }
            };

            processed.insert(id, processed_resource);
        }

        Ok(processed)
    }

    fn save_resource(
        &self,
        document_path: &Path,
        resource: &RawResource,
    ) -> Result<PathBuf, Error> {
        let document_dir = document_path
            .parent()
            .ok_or_else(|| Error::InvalidDocument("document has no parent directory".into()))?;

        let resource_dir = document_dir
            .join(".resources")
            .join(generate_resource_directory_name(document_path));

        fs::create_dir_all(&resource_dir)?;

        let extension = mime_extension(&resource.mime_type);

        // Resource IDs originate in document contents and may contain path
        // separators or traversal components. Never use them as filesystem paths.
        let safe_id = hex::encode(Sha256::digest(resource.id.as_bytes()));
        let file_name = if extension.is_empty() {
            safe_id
        } else {
            format!("{}.{}", safe_id, extension,)
        };

        let path = resource_dir.join(file_name);

        if !path.exists() {
            fs::write(&path, &resource.data)?;
        }

        Ok(path)
    }
}

#[derive(Debug, Default)]
struct ProcessedResources {
    resources: HashMap<String, ProcessedResource>,
}

impl ProcessedResources {
    fn insert(&mut self, id: String, resource: ProcessedResource) {
        self.resources.insert(id, resource);
    }
}

#[derive(Debug)]
enum ProcessedResource {
    Source(String),
    None,
}

fn find_resource<'a>(resources: &'a [RawResource], id: &str) -> Option<&'a RawResource> {
    resources.iter().find(|resource| resource.id == id)
}

fn replace_resource_references(html: &str, resources: &ProcessedResources) -> String {
    let mut result = html.to_owned();

    for (id, resource) in &resources.resources {
        match resource {
            ProcessedResource::Source(source) => {
                result = result.replace(&format!("src=\"{}\"", id), &format!("src=\"{}\"", source));

                result = result.replace(&format!("src='{}'", id), &format!("src='{}'", source));

                result =
                    result.replace(&format!("href=\"{}\"", id), &format!("href=\"{}\"", source));

                result = result.replace(&format!("href='{}'", id), &format!("href='{}'", source));
            }

            ProcessedResource::None => {
                result = result.replace(&format!("src=\"{}\"", id), "");

                result = result.replace(&format!("src='{}'", id), "");

                result = result.replace(&format!("href=\"{}\"", id), "");

                result = result.replace(&format!("href='{}'", id), "");
            }
        }
    }

    result
}

fn detect_document_language_with_limit(chapters: &[Chapter], max_chars: usize) -> Option<String> {
    if chapters.is_empty() {
        return None;
    }

    if max_chars == 0 {
        return detect_document_language(chapters);
    }

    let mut chapters_for_detection = Vec::new();
    let mut remaining = max_chars;

    for chapter in chapters {
        if remaining == 0 {
            break;
        }

        let text = match &chapter.content {
            ChapterContent::Text(text) => text.clone(),

            ChapterContent::Html(html) => html_to_text(html),
        };

        let text = text.chars().take(remaining).collect::<String>();

        remaining = remaining.saturating_sub(text.chars().count());

        if !text.is_empty() {
            chapters_for_detection.push(Chapter {
                title: chapter.title.clone(),
                content: ChapterContent::Text(text),
            });
        }
    }

    if chapters_for_detection.is_empty() {
        return None;
    }

    detect_document_language(&chapters_for_detection)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{RawChapter, RawMetadata};

    #[test]
    fn sanitized_html_keeps_generated_base64_images() {
        let options = ParseOptions {
            image_load: ImageLoadType::Base64,
            sanitize_html: true,
            ..Default::default()
        };
        let raw = RawDocument {
            metadata: RawMetadata {
                title: Some("image test".into()),
                author: None,
                description: None,
                language: None,
                cover_id: Some("cover".into()),
            },
            chapters: vec![RawChapter {
                title: None,
                content: r#"<p><img src="cover" alt="cover"></p>"#.into(),
            }],
            resources: vec![RawResource {
                id: "cover".into(),
                mime_type: "image/png".into(),
                data: vec![1, 2, 3],
            }],
        };

        let document = DocumentParser::new()
            .with_options(options)
            .finalize(raw, Path::new("book.epub"))
            .expect("finalization should succeed");

        let ChapterContent::Html(html) = &document.content.chapters[0].content else {
            panic!("expected HTML content");
        };
        assert!(
            html.contains("src=\"data:image/png;base64,AQID\""),
            "{html}"
        );
        assert_eq!(document.metadata.cover.as_deref(), Some(&[1, 2, 3][..]));
    }
}
