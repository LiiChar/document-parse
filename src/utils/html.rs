use html2text::from_read;

pub fn html_to_text(html: &str) -> String {
    from_read(html.as_bytes(), 80)
        .unwrap_or_default()
        .replace('\u{00A0}', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn sanitize_html(html: &str) -> String {
    ammonia::Builder::default()
        .add_tags(["img", "h1", "h2", "h3", "h4"])
        .clean(html)
        .to_string()
}

pub fn normalize_html_whitespace(html: &str) -> String {
    html.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}
