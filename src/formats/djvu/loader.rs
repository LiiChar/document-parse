// Альфа версия загрузчика, необходимо будет переделать

use rayon::prelude::*;
use std::sync::mpsc;
use std::{fs, path::Path, sync::Arc};

use djvu::Document;
use image::{ImageBuffer, Rgba, codecs::jpeg::JpegEncoder};

use crate::{
    error::Error,
    model::{RawChapter, RawDocument, RawMetadata, RawResource},
    parser::{Loader, ParseOptions},
    utils::text::{escape_html, text_to_html},
};

pub struct DjvuLoader;

impl Loader for DjvuLoader {
    fn supports(&self, path: &Path) -> bool {
        path.extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.eq_ignore_ascii_case("djvu"))
            .unwrap_or(false)
    }

    fn load(&self, path: &Path, _options: &ParseOptions) -> Result<RawDocument, Error> {
        let document = Document::open(path).map_err(|error| {
            Error::Parser(format!(
                "failed to open DJVU '{}': {}",
                path.display(),
                error,
            ))
        })?;

        let pages = document.page_count();

        let title = extract_title(path);

        // Если страниц мало - обрабатываем последовательно
        if pages <= 10 {
            return self.load_small_document(&document, pages, title);
        }

        // Для больших документов используем параллельную обработку
        self.load_large_document(&document, pages, title)
    }
}

impl DjvuLoader {
    /// Загрузка маленького документа (последовательно)
    fn load_small_document(
        &self,
        document: &Document,
        pages: usize,
        title: String,
    ) -> Result<RawDocument, Error> {
        let mut chapters = Vec::with_capacity(pages);
        let mut resources = Vec::new();

        for index in 0..pages {
            let page = document.page(index).map_err(|error| {
                Error::Parser(format!("failed to open DJVU page {}: {}", index, error))
            })?;

            self.process_page(&page, index, &mut chapters, &mut resources)?;
        }

        let cover_id = resources.first().map(|resource| resource.id.clone());

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

    /// Загрузка большого документа (параллельно)
    fn load_large_document(
        &self,
        document: &Document,
        pages: usize,
        title: String,
    ) -> Result<RawDocument, Error> {
        // Создаем Arc для разделения документа между потоками
        let document_arc = Arc::new(document.clone());

        // Создаем каналы для сбора результатов
        let (tx, rx) = mpsc::channel();

        // Параллельно обрабатываем страницы
        (0..pages).into_par_iter().for_each(|index| {
            let tx = tx.clone();
            let document = document_arc.clone();

            let result = std::panic::catch_unwind(|| self.process_page_parallel(&document, index));

            match result {
                Ok(Ok((chapter, resource))) => {
                    let _ = tx.send(Ok((index, chapter, resource)));
                }
                Ok(Err(e)) => {
                    let _ = tx.send(Err(e));
                }
                Err(_) => {
                    let _ = tx.send(Err(Error::Parser(format!("Panic in page {}", index))));
                }
            }
        });

        drop(tx); // Закрываем канал

        // Собираем результаты
        let mut chapters = vec![None; pages];
        let mut resources = Vec::new();

        for result in rx {
            match result {
                Ok((index, chapter, resource)) => {
                    chapters[index] = Some(chapter);
                    if let Some(res) = resource {
                        resources.push(res);
                    }
                }
                Err(e) => {
                    return Err(e);
                }
            }
        }

        // Преобразуем Option в RawChapter
        let chapters: Vec<RawChapter> = chapters
            .into_iter()
            .enumerate()
            .map(|(index, opt)| {
                opt.unwrap_or_else(|| RawChapter {
                    title: Some(format!("Page {}", index + 1)),
                    content: format!("<p>Error loading page {}</p>", index + 1),
                })
            })
            .collect();

        let cover_id = resources.first().map(|resource| resource.id.clone());

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

    /// Обработка одной страницы (последовательная версия)
    fn process_page(
        &self,
        page: &djvu::Page,
        index: usize,
        chapters: &mut Vec<RawChapter>,
        resources: &mut Vec<RawResource>,
    ) -> Result<(), Error> {
        // Пытаемся получить текст
        let text_result = page
            .text()
            .map_err(|error| Error::Parser(format!("failed to parse text DJVU: {}", error)))?;

        if let Some(text) = text_result {
            if !text.trim().is_empty() {
                chapters.push(RawChapter {
                    title: Some(format!("Page {}", index + 1)),
                    content: text_to_html(&text),
                });
                return Ok(());
            }
        }

        // Текст отсутствует - рендерим только первые 3 страницы
        const MAX_RENDER_PAGES: usize = 3;

        if index < MAX_RENDER_PAGES {
            let resource_id = format!("page-{}.jpg", index + 1);

            match render_page_to_jpeg(page, index) {
                Ok(image_data) => {
                    resources.push(RawResource {
                        id: resource_id.clone(),
                        mime_type: "image/jpeg".to_owned(),
                        data: image_data,
                    });

                    chapters.push(RawChapter {
                        title: Some(format!("Page {}", index + 1)),
                        content: format!(
                            r#"<p><img src="{}" alt="Page {}" loading="lazy" /></p>"#,
                            escape_html(&resource_id),
                            index + 1,
                        ),
                    });
                }
                Err(error) => {
                    chapters.push(RawChapter {
                        title: Some(format!("Page {}", index + 1)),
                        content: format!("<p>Page {}</p>", index + 1),
                    });
                }
            }
        } else {
            chapters.push(RawChapter {
                title: Some(format!("Page {}", index + 1)),
                content: format!("<p>Page {}</p>", index + 1),
            });
        }

        Ok(())
    }

    /// Обработка одной страницы (параллельная версия)
    fn process_page_parallel(
        &self,
        document: &Arc<&Document>,
        index: usize,
    ) -> Result<(RawChapter, Option<RawResource>), Error> {
        let page = document.page(index).map_err(|error| {
            Error::Parser(format!("failed to open DJVU page {}: {}", index, error))
        })?;

        // Пытаемся получить текст
        let text_result = page
            .text()
            .map_err(|error| Error::Parser(format!("failed to parse text DJVU: {}", error)))?;

        if let Some(text) = text_result {
            if !text.trim().is_empty() {
                return Ok((
                    RawChapter {
                        title: Some(format!("Page {}", index + 1)),
                        content: text_to_html(&text),
                    },
                    None,
                ));
            }
        }

        // Текст отсутствует - рендерим только первые 3 страницы
        const MAX_RENDER_PAGES: usize = 3;

        if index < MAX_RENDER_PAGES {
            let resource_id = format!("page-{}.jpg", index + 1);

            match render_page_to_jpeg(&page, index) {
                Ok(image_data) => {
                    let resource = RawResource {
                        id: resource_id.clone(),
                        mime_type: "image/jpeg".to_owned(),
                        data: image_data,
                    };

                    let chapter = RawChapter {
                        title: Some(format!("Page {}", index + 1)),
                        content: format!(
                            r#"<p><img src="{}" alt="Page {}" loading="lazy" /></p>"#,
                            escape_html(&resource_id),
                            index + 1,
                        ),
                    };

                    return Ok((chapter, Some(resource)));
                }
                Err(error) => {}
            }
        }

        Ok((
            RawChapter {
                title: Some(format!("Page {}", index + 1)),
                content: format!("<p>Page {}</p>", index + 1),
            },
            None,
        ))
    }
}

/// Извлекает название документа из имени файла
fn extract_title(path: &Path) -> String {
    path.file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("Untitled")
        .trim()
        .to_owned()
}

/// Рендерит страницу в JPEG с ограничением размера
fn render_page_to_jpeg(page: &djvu::Page, index: usize) -> Result<Vec<u8>, Error> {
    // Ограничиваем максимальный размер изображения
    const MAX_WIDTH: u32 = 800;
    const MAX_HEIGHT: u32 = 1200;
    const JPEG_QUALITY: u8 = 70;

    let pixmap = page
        .render_to_size(MAX_WIDTH, MAX_HEIGHT)
        .map_err(|error| {
            Error::Parser(format!("failed to render DJVU page {}: {}", index, error))
        })?;

    let rgba_bytes = pixmap.as_ref();

    // Конвертируем в JPEG
    let image = ImageBuffer::<Rgba<u8>, _>::from_raw(MAX_WIDTH, MAX_HEIGHT, rgba_bytes.to_vec())
        .ok_or_else(|| Error::Parser(format!("failed to create image from page {}", index)))?;

    let mut jpeg_data = Vec::new();
    let mut encoder = JpegEncoder::new_with_quality(&mut jpeg_data, JPEG_QUALITY);

    encoder.encode_image(&image).map_err(|error| {
        Error::Parser(format!(
            "failed to encode JPEG for page {}: {}",
            index, error
        ))
    })?;

    Ok(jpeg_data)
}

// Вспомогательные функции сохранены для совместимости
#[allow(dead_code)]
fn is_image(name: &str) -> bool {
    let extension = Path::new(name)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase());

    matches!(
        extension.as_deref(),
        Some("jpg" | "jpeg" | "png" | "webp" | "gif" | "bmp" | "avif")
    )
}

#[allow(dead_code)]
fn guess_mime(name: &str) -> &'static str {
    let extension = Path::new(name)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase());

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
