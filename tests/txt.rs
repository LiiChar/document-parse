mod common;

use document_parser::{
    model::ChapterContent,
    parser::{ContentType, DocumentParser, ParseOptions},
};

use common::TestFile;

#[test]
fn parses_utf8_txt() {
    let file = TestFile::new("book.txt", "Hello world.\n\nThis is a book.");

    let document = DocumentParser::new()
        .parse(&file.path)
        .expect("failed to parse");

    assert_eq!(document.metadata.title, "book");

    assert_eq!(document.content.chapters.len(), 1);

    match &document.content.chapters[0].content {
        ChapterContent::Html(html) => {
            assert!(html.contains("<p>"));

            assert!(html.contains("Hello world."));
        }

        _ => panic!("expected HTML content"),
    }
}

#[test]
fn txt_can_return_plain_text() {
    let file = TestFile::new("book.txt", "Hello world.\n\nSecond paragraph.");

    let options = ParseOptions {
        content_type: ContentType::Text,
        ..Default::default()
    };

    let document = DocumentParser::new()
        .with_options(options)
        .parse(&file.path)
        .expect("failed to parse");

    match &document.content.chapters[0].content {
        ChapterContent::Text(text) => {
            assert_eq!(text, "Hello world.\n\nSecond paragraph.");
        }

        _ => panic!("expected text content"),
    }
}

#[test]
fn txt_splits_chapters() {
    let file = TestFile::new("book.txt", "Chapter 1\n\nText.\n\nChapter 2\n\nText.");

    let options = ParseOptions {
        split_txt_chapters: true,
        ..Default::default()
    };

    let document = DocumentParser::new()
        .with_options(options)
        .parse(&file.path)
        .expect("failed to parse");

    assert!(document.content.chapters.len() >= 2);
}

#[test]
fn txt_handles_cyrillic() {
    let file = TestFile::new("книга.txt", "Привет мир.\n\nЭто текст книги.");

    let document = DocumentParser::new()
        .parse(&file.path)
        .expect("failed to parse");

    assert_eq!(document.metadata.title, "книга");

    assert!(document.metadata.language.is_some());
}
