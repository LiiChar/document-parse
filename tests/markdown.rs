mod common;

use document_parser::{model::ChapterContent, parser::DocumentParser};

use common::TestFile;

#[test]
fn markdown_is_converted_to_html() {
    let file = TestFile::new(
        "book.md",
        r#"
# Chapter 1

Hello **world**.

- One
- Two

~~deleted~~
"#,
    );

    let document = DocumentParser::new()
        .parse(&file.path)
        .expect("failed to parse");

    let html = match &document.content.chapters[0].content {
        ChapterContent::Html(html) => html,

        _ => panic!("expected HTML"),
    };

    assert!(html.contains("<h1>"));

    assert!(html.contains("<strong>world</strong>"));

    assert!(html.contains("<ul>"));

    assert!(html.contains("<del>") || html.contains("<s>"));
}
