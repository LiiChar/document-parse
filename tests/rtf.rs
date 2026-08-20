mod common;

use document_parser::{model::ChapterContent, parser::DocumentParser};

use common::TestFile;

#[test]
fn rtf_preserves_basic_formatting() {
    let rtf = r#"{\rtf1\ansi
This is \b bold\b0  and \i italic\i0 .
}"#;

    let file = TestFile::new("book.rtf", rtf);

    let document = DocumentParser::new()
        .parse(&file.path)
        .expect("failed to parse");

    let html = match &document.content.chapters[0].content {
        ChapterContent::Html(html) => html,
        _ => panic!("expected HTML"),
    };

    assert!(html.contains("<strong>"));

    assert!(html.contains("<em>"));
}

#[test]
fn rtf_handles_cyrillic() {
    let rtf = r#"{\rtf1\ansi\ansicpg1251
{\fonttbl{\f0 Arial;}}
\f0 Привет мир.
}"#;

    let file = TestFile::new("russian.rtf", rtf);

    let document = DocumentParser::new()
        .parse(&file.path)
        .expect("failed to parse");

    let html = match &document.content.chapters[0].content {
        ChapterContent::Html(html) => html,
        _ => panic!("expected HTML"),
    };

    assert!(html.contains("Привет"));
}
