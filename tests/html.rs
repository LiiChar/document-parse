mod common;


use common::TestFile;
use document_parser::{DocumentParser, ParseOptions, model::ChapterContent};

#[test]
fn html_is_preserved() {
    let file = TestFile::new(
        "book.html",
        r#"
<html>
<head>
    <title>My Book</title>
</head>
<body>
    <h1>Hello</h1>
    <p>World</p>
</body>
</html>
"#,
    );

    let document =
        DocumentParser::new()
            .parse(&file.path)
            .expect("failed to parse");

    assert_eq!(
        document.metadata.title,
        "My Book"
    );

    match &document.content.chapters[0].content {
        ChapterContent::Html(html) => {
            assert!(
                html.contains("<h1>")
            );
        }

        _ => panic!("expected HTML"),
    }
}

#[test]
fn html_sanitization_removes_script() {
    let file = TestFile::new(
        "malicious.html",
        r#"
<html>
<body>
<script>alert("x")</script>
<p>Hello</p>
</body>
</html>
"#,
    );

    let options =
        ParseOptions {
            sanitize_html: true,
            ..Default::default()
        };

    let document =
        DocumentParser::new()
            .with_options(options)
            .parse(&file.path)
            .expect("failed to parse");

    let html =
        match &document.content.chapters[0].content {
            ChapterContent::Html(html) => html,
            _ => panic!("expected HTML"),
        };

    assert!(
        !html.contains("<script")
    );

    assert!(
        html.contains("Hello")
    );
}
