use std::{
    fs::File,
    io::Read,
    path::Path,
};

use natord::compare;
use zip::ZipArchive;

use crate::{
    error::Error,
    model::{
        RawChapter,
        RawDocument,
        RawMetadata,
        RawResource,
    },
    parser::{Loader, ParseOptions},
    utils::text::escape_html,
};

pub struct CbzLoader;

impl Loader for CbzLoader {
    fn supports(&self, path: &Path) -> bool {
        path.extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                extension.eq_ignore_ascii_case("cbz")
            })
    }

    fn load(
        &self,
        path: &Path,
        _options: &ParseOptions,
    ) -> Result<RawDocument, Error> {
        let file = File::open(path)?;

        let mut archive = ZipArchive::new(file)
            .map_err(|error| {
                Error::Parser(format!(
                    "failed to open CBZ '{}': {}",
                    path.display(),
                    error,
                ))
            })?;

        let mut pages = Vec::new();

        for index in 0..archive.len() {
            let mut entry = archive
                .by_index(index)
                .map_err(|error| {
                    Error::Parser(format!(
                        "failed to read CBZ entry {}: {}",
                        index,
                        error,
                    ))
                })?;

            if entry.is_dir() {
                continue;
            }

            let name = entry.name().to_owned();

            if !is_image(&name) {
                continue;
            }

            let mut data = Vec::new();

            entry
                .read_to_end(&mut data)
                .map_err(|error| {
                    Error::Parser(format!(
                        "failed to read CBZ image '{}': {}",
                        name,
                        error,
                    ))
                })?;

            if data.is_empty() {
                continue;
            }

            pages.push(
                Page {
                    name,
                    data,
                },
            );
        }

        pages.sort_by(|left, right| {
            compare(
                &left.name,
                &right.name,
            )
        });

        if pages.is_empty() {
            return Err(Error::InvalidDocument(
                "CBZ archive does not contain images"
                    .to_owned(),
            ));
        }

        let title = extract_title(path);

        let mut chapters =
            Vec::with_capacity(pages.len());

        let mut resources =
            Vec::with_capacity(pages.len());

        for (index, page) in pages.iter().enumerate() {
            let resource_id =
                format!("page-{}", index + 1);

            resources.push(
                RawResource {
                    id: resource_id.clone(),
                    mime_type: guess_mime(
                        &page.name,
                    )
                    .to_owned(),
                    data: page.data.clone(),
                },
            );

            chapters.push(
                RawChapter {
                    title: Some(
                        format!(
                            "Page {}",
                            index + 1,
                        ),
                    ),
                    content: format!(
                        "<p><img src=\"{}\" alt=\"Page {}\" /></p>",
                        escape_html(&resource_id),
                        index + 1,
                    ),
                },
            );
        }

        let cover_id =
            resources
                .first()
                .map(|resource| resource.id.clone());

        Ok(RawDocument {
            metadata: RawMetadata {
                title: Some(title),
                author: None,
                description: None,
                language: None,
                cover_id,
            },
            chapters,
            resources,
        })
    }
}

#[derive(Debug)]
struct Page {
    name: String,
    data: Vec<u8>,
}

fn extract_title(path: &Path) -> String {
    path.file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("Untitled")
        .trim()
        .to_owned()
}

fn is_image(name: &str) -> bool {
    let extension = Path::new(name)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| {
            extension.to_ascii_lowercase()
        });

    matches!(
        extension.as_deref(),
        Some(
            "jpg"
                | "jpeg"
                | "png"
                | "webp"
                | "gif"
                | "bmp"
                | "avif"
        )
    )
}

fn guess_mime(name: &str) -> &'static str {
    let extension = Path::new(name)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| {
            extension.to_ascii_lowercase()
        });

    match extension.as_deref() {
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        Some("gif") => "image/gif",
        Some("bmp") => "image/bmp",
        Some("avif") => "image/avif",
        _ => "application/octet-stream",
    }
}
