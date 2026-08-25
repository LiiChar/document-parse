use std::sync::OnceLock;

use encoding_rs::WINDOWS_1251;
use regex::Regex;

use crate::model::{Chapter, ChapterContent};

const MAX_TITLE_LENGTH: usize = 120;
const MAX_TITLE_WORDS: usize = 15;

#[derive(Debug, Clone)]
struct ChapterBoundary {
    line_index: usize,
    title: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HeadingKind {
    Roman,
    Numbered,
    Explicit,
    Part,
    Special,
}

/// Разделяет TXT-документ на главы.
///
/// Основной принцип:
/// - TXT не имеет надёжной структуры, поэтому нельзя агрессивно
///   считать любую короткую строку заголовком.
/// - Приоритет имеют явные конструкции:
///
///     Глава I. Название
///     Глава 1. Название
///     I. Название
///     1. Название
///     Часть первая
///     Chapter 1
///
/// - Обычные предложения и реплики никогда не считаются главами.
/// - Если структура глав не подтверждается несколькими заголовками,
///   весь документ остаётся одной главой.
pub fn split_into_chapters(text: &str) -> Vec<Chapter> {
    let text = normalize_source_text(text);

    if text.trim().is_empty() {
        return Vec::new();
    }

    let lines: Vec<&str> = text.lines().collect();

    if lines.is_empty() {
        return Vec::new();
    }

    let boundaries = detect_chapter_boundaries(&lines);

    if boundaries.is_empty() {
        return vec![Chapter {
            title: None,
            content: ChapterContent::Html(text_to_html(&text)),
        }];
    }

    build_chapters_from_boundaries(&lines, &boundaries)
}

/* -------------------------------------------------------------------------- */
/* Chapter detection                                                          */
/* -------------------------------------------------------------------------- */

fn detect_chapter_boundaries(lines: &[&str]) -> Vec<ChapterBoundary> {
    let mut candidates = Vec::new();

    for (index, raw_line) in lines.iter().enumerate() {
        let line = normalize_heading_line(raw_line);

        if line.is_empty() {
            continue;
        }

        let Some((title, kind)) = detect_title(&line) else {
            continue;
        };

        candidates.push((index, title, kind));
    }

    if candidates.is_empty() {
        return Vec::new();
    }

    /*
     * Самая важная часть алгоритма.
     *
     * Один случайный "I. ..." ничего не значит.
     *
     * Для настоящей книги обычно встречается последовательность:
     *
     * I. ...
     * II. ...
     * III. ...
     * IV. ...
     *
     * Поэтому мы сначала ищем устойчивую структуру.
     */

    let mut confirmed = Vec::new();

    for (position, candidate) in candidates.iter().enumerate() {
        let (index, title, kind) = candidate;

        let previous = candidates.get(position.wrapping_sub(1));
        let next = candidates.get(position + 1);

        let previous_compatible = previous
            .map(|(_, _, previous_kind)| compatible_heading_kinds(*previous_kind, *kind))
            .unwrap_or(false);

        let next_compatible = next
            .map(|(_, _, next_kind)| compatible_heading_kinds(*next_kind, *kind))
            .unwrap_or(false);

        let has_structure = previous_compatible || next_compatible;

        /*
         * Явные "Глава N" считаем достаточно надёжными сами по себе.
         *
         * А вот:
         *
         * "Потом он крикнул:"
         * "– Он скончался."
         *
         * сюда не попадут вообще.
         */
        let strong = matches!(
            kind,
            HeadingKind::Explicit | HeadingKind::Part | HeadingKind::Special
        );

        if strong || has_structure {
            confirmed.push(ChapterBoundary {
                line_index: *index,
                title: title.clone(),
            });
        }
    }

    /*
     * Для римских/числовых заголовков дополнительно проверяем,
     * что есть хотя бы две главы.
     *
     * Это очень важный фильтр против случайных:
     *
     * XIII. Что-то
     *
     * внутри обычного текста.
     */
    if confirmed.len() < 2 {
        let strong_count = candidates
            .iter()
            .filter(|(_, _, kind)| {
                matches!(
                    kind,
                    HeadingKind::Explicit | HeadingKind::Part | HeadingKind::Special
                )
            })
            .count();

        if strong_count == 0 {
            return Vec::new();
        }
    }

    /*
     * Убираем дубликаты.
     */
    let mut result = Vec::new();

    for boundary in confirmed {
        if result
            .last()
            .map(|last: &ChapterBoundary| last.line_index == boundary.line_index)
            .unwrap_or(false)
        {
            continue;
        }

        result.push(boundary);
    }

    /*
     * Если первая глава начинается слишком далеко от начала,
     * скорее всего найден случайный заголовок.
     *
     * Но допускаем большой preface / аннотацию.
     */
    if let Some(first) = result.first() {
        let non_empty_lines = lines.iter().filter(|line| !line.trim().is_empty()).count();

        if non_empty_lines > 20 {
            let first_position = lines[..first.line_index]
                .iter()
                .filter(|line| !line.trim().is_empty())
                .count();

            /*
             * Если больше 60% книги прошло до первого заголовка —
             * это почти наверняка ложное распознавание.
             */
            if first_position > non_empty_lines * 6 / 10 {
                return Vec::new();
            }
        }
    }

    result
}

fn compatible_heading_kinds(a: HeadingKind, b: HeadingKind) -> bool {
    match (a, b) {
        (HeadingKind::Roman, HeadingKind::Roman) => true,
        (HeadingKind::Numbered, HeadingKind::Numbered) => true,

        /*
         * Некоторые книги имеют:
         *
         * Часть первая
         * I. ...
         *
         * поэтому Part/Explicit можем связывать с numbered/roman.
         */
        (HeadingKind::Part, HeadingKind::Part) => true,
        (HeadingKind::Part, HeadingKind::Roman)
        | (HeadingKind::Roman, HeadingKind::Part)
        | (HeadingKind::Part, HeadingKind::Numbered)
        | (HeadingKind::Numbered, HeadingKind::Part) => true,

        (HeadingKind::Explicit, HeadingKind::Explicit) => true,

        _ => false,
    }
}

fn detect_title(text: &str) -> Option<(String, HeadingKind)> {
    let text = normalize_heading_line(text);

    if text.is_empty() {
        return None;
    }

    if text.chars().count() > MAX_TITLE_LENGTH {
        return None;
    }

    if text.split_whitespace().count() > MAX_TITLE_WORDS {
        return None;
    }

    /*
     * Важный фильтр:
     *
     * Заголовок не должен быть репликой.
     */
    if looks_like_dialogue(&text) {
        return None;
    }

    /*
     * И не должен выглядеть как обычное предложение.
     */
    if looks_like_sentence(&text) {
        /*
         * Исключение:
         *
         * "Глава I. Что случилось?"
         *
         * Это всё ещё заголовок.
         */
        if !is_explicit_heading(&text) {
            return None;
        }
    }

    if let Some(title) = parse_explicit_heading(&text) {
        return Some((title, HeadingKind::Explicit));
    }

    if let Some(title) = parse_part_heading(&text) {
        return Some((title, HeadingKind::Part));
    }

    if let Some(title) = parse_roman_heading(&text) {
        return Some((title, HeadingKind::Roman));
    }

    if let Some(title) = parse_numbered_heading(&text) {
        return Some((title, HeadingKind::Numbered));
    }

    if is_special_heading(&text) {
        return Some((clean_title(&text), HeadingKind::Special));
    }

    None
}

/* -------------------------------------------------------------------------- */
/* Explicit headings                                                          */
/* -------------------------------------------------------------------------- */

fn parse_explicit_heading(text: &str) -> Option<String> {
    static RE: OnceLock<Regex> = OnceLock::new();

    let re = RE.get_or_init(|| {
        Regex::new(
            r"(?iu)^\s*\
            (?:chapter|chap\.?|глава|гл\.?)\
            \s+\
            (?:\d{1,4}|[ivxlcdm]{1,8}|[a-zа-яё]+)\
            (?:\s*[\.\-:)]\s*|\s+)\
            (.+?)\
            \s*$",
        )
        .expect("valid explicit heading regex")
    });

    let captures = re.captures(text)?;

    let title = captures.get(0)?.as_str();

    Some(clean_title(title))
}

fn is_explicit_heading(text: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();

    let re = RE.get_or_init(|| {
        Regex::new(
            r"(?iu)^\s*(?:chapter|chap\.?|глава|гл\.?)\s+(?:\d{1,4}|[ivxlcdm]{1,8}|[a-zа-яё]+)",
        )
        .expect("valid explicit heading detection regex")
    });

    re.is_match(text)
}

/* -------------------------------------------------------------------------- */
/* Part headings                                                              */
/* -------------------------------------------------------------------------- */

fn parse_part_heading(text: &str) -> Option<String> {
    static RE: OnceLock<Regex> = OnceLock::new();

    let re = RE.get_or_init(|| {
        Regex::new(
            r"(?iu)^\s*(?:part|часть|book|книга|section|раздел|секция)\
            \s+(?:\d{1,4}|[ivxlcdm]{1,8}|[a-zа-яё]+)\
            (?:\s*[\.\-:)]\s*.*)?$",
        )
        .expect("valid part heading regex")
    });

    if re.is_match(text) {
        Some(clean_title(text))
    } else {
        None
    }
}

/* -------------------------------------------------------------------------- */
/* Roman headings                                                             */
/* -------------------------------------------------------------------------- */

fn parse_roman_heading(text: &str) -> Option<String> {
    static RE: OnceLock<Regex> = OnceLock::new();

    let re = RE.get_or_init(|| {
        Regex::new(
            r"(?iu)^\s*
            (?P<number>[ivxlcdm]{1,8})
            \s*[\.\-:)]
            \s+
            (?P<title>[^\d].{1,119})
            $",
        )
        .expect("valid roman heading regex")
    });

    let captures = re.captures(text)?;

    let title = captures.name("title")?.as_str().trim();

    if title.is_empty() || looks_like_sentence(title) {
        return None;
    }

    /*
     * "I. Марсель. Прибытие" — OK
     *
     * "I. Это было ..." — может быть обычный текст,
     * поэтому дополнительно запрещаем полноценные предложения.
     */
    if looks_like_sentence(title) {
        return None;
    }

    Some(clean_title(text))
}

/* -------------------------------------------------------------------------- */
/* Numeric headings                                                           */
/* -------------------------------------------------------------------------- */

fn parse_numbered_heading(text: &str) -> Option<String> {
    static RE: OnceLock<Regex> = OnceLock::new();

    let re = RE.get_or_init(|| {
        Regex::new(
            r"(?iu)^\s*(?P<number>\d{1,4})\s*[\.\-:)]\s+(?P<title>[^\d].{1,119})$",
        )
        .expect("valid numbered heading regex")
    });

    let captures = re.captures(text)?;

    let title = captures.name("title")?.as_str().trim();

    if title.is_empty() || looks_like_sentence(title) {
        return None;
    }

    Some(clean_title(text))
}

/* -------------------------------------------------------------------------- */
/* Special headings                                                           */
/* -------------------------------------------------------------------------- */

fn is_special_heading(text: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();

    let re = RE.get_or_init(|| {
        Regex::new(
            r"(?iu)^\s*
            (?:пролог|эпилог|введение|предисловие|послесловие|приложение|
               prologue|epilogue|introduction|foreword|afterword|appendix)
            (?:\s*[:\-—].*)?
            \s*$",
        )
        .expect("valid special heading regex")
    });

    re.is_match(text)
}

/* -------------------------------------------------------------------------- */
/* Text heuristics                                                            */
/* -------------------------------------------------------------------------- */

fn looks_like_dialogue(text: &str) -> bool {
    let trimmed = text.trim_start();

    /*
     * Русские тире:
     *
     * – А!
     * — Что случилось?
     *
     * Никогда не считаем это заголовком.
     */
    trimmed.starts_with('–')
        || trimmed.starts_with('—')
        || trimmed.starts_with('-')
        || trimmed.starts_with('―')
}

fn looks_like_sentence(text: &str) -> bool {
    let text = text.trim();

    if text.is_empty() {
        return false;
    }

    /*
     * Явная пунктуация конца предложения.
     */
    if text.ends_with('.')
        || text.ends_with('!')
        || text.ends_with('?')
        || text.ends_with('…')
        || text.ends_with(';')
    {
        return true;
    }

    let words = text.split_whitespace().count();

    /*
     * Очень короткие заголовки:
     *
     * "Марсель"
     * "Предисловие"
     * "Признание"
     *
     * не должны попадать сюда.
     */
    if words <= 3 {
        return false;
    }

    /*
     * Для длинной строки вероятность того, что это обычное
     * предложение, резко возрастает.
     */
    if words >= 10 {
        return true;
    }

    let lower = text.to_lowercase();

    /*
     * Частые русские конструкции обычного предложения.
     */
    let russian_sentence_markers = [
        " и ",
        " а ",
        " но ",
        " это ",
        " был ",
        " была ",
        " были ",
        " будет ",
        " будут ",
        " есть ",
        " его ",
        " её ",
        " они ",
        " она ",
        " он ",
        " мы ",
        " вы ",
        " я ",
        " мне ",
        " ему ",
        " ей ",
        " когда ",
        " потому ",
        " чтобы ",
        " который ",
        " которая ",
        " которые ",
    ];

    if russian_sentence_markers
        .iter()
        .any(|marker| lower.contains(marker))
    {
        return true;
    }

    /*
     * Английские конструкции.
     */
    let english_sentence_markers = [
        " and ",
        " but ",
        " this ",
        " that ",
        " was ",
        " were ",
        " have ",
        " has ",
        " had ",
        " will ",
        " when ",
        " which ",
        " who ",
        " they ",
        " he ",
        " she ",
    ];

    english_sentence_markers
        .iter()
        .any(|marker| lower.contains(marker))
}

fn is_title_case(text: &str) -> bool {
    let words: Vec<&str> = text.split_whitespace().collect();

    if words.is_empty() {
        return false;
    }

    let mut meaningful = 0;
    let mut uppercase = 0;

    for word in words {
        let clean = word.trim_matches(|c: char| !c.is_alphabetic());

        if clean.is_empty() {
            continue;
        }

        meaningful += 1;

        if clean
            .chars()
            .next()
            .map(|c| c.is_uppercase())
            .unwrap_or(false)
        {
            uppercase += 1;
        }
    }

    meaningful > 0 && uppercase as f32 / meaningful as f32 >= 0.6
}

fn is_all_caps(text: &str) -> bool {
    let letters: Vec<char> = text.chars().filter(|c| c.is_alphabetic()).collect();

    if letters.len() < 3 {
        return false;
    }

    letters.iter().all(|c| !c.is_lowercase())
}

/* -------------------------------------------------------------------------- */
/* Building chapters                                                          */
/* -------------------------------------------------------------------------- */

fn build_chapters_from_boundaries(
    lines: &[&str],
    boundaries: &[ChapterBoundary],
) -> Vec<Chapter> {
    let mut chapters = Vec::new();

    for (position, boundary) in boundaries.iter().enumerate() {
        let start = boundary.line_index + 1;

        let end = boundaries
            .get(position + 1)
            .map(|next| next.line_index)
            .unwrap_or(lines.len());

        if start >= end {
            continue;
        }

        let content = lines[start..end].join("\n");

        if content.trim().is_empty() {
            continue;
        }

        chapters.push(Chapter {
            title: Some(boundary.title.clone()),
            content: ChapterContent::Html(text_to_html(&content)),
        });
    }

    /*
     * Текст до первой главы:
     *
     * Аннотация
     * Автор
     * Издательство
     * ...
     *
     * оставляем отдельным preface.
     */
    if let Some(first) = boundaries.first() {
        let preface = lines[..first.line_index].join("\n");

        if !preface.trim().is_empty() {
            chapters.insert(
                0,
                Chapter {
                    title: Some("Annotation".to_string()),
                    content: ChapterContent::Html(text_to_html(&preface)),
                },
            );
        }
    }

    chapters
}

/* -------------------------------------------------------------------------- */
/* HTML                                                                       */
/* -------------------------------------------------------------------------- */

pub fn text_to_html(text: &str) -> String {
    let text = normalize_line_endings(text);

    if text.trim().is_empty() {
        return String::new();
    }

    /*
     * Если TXT на самом деле содержит HTML,
     * не экранируем его второй раз.
     */
    if looks_like_html(&text) {
        return normalize_embedded_html(&text);
    }

    let mut html = String::new();
    let mut paragraph: Vec<&str> = Vec::new();

    for line in text.lines() {
        let line = line.trim();

        if line.is_empty() {
            flush_paragraph(&mut html, &mut paragraph);
            continue;
        }

        paragraph.push(line);
    }

    flush_paragraph(&mut html, &mut paragraph);

    html
}

fn normalize_embedded_html(text: &str) -> String {
    let mut result = text.trim().to_string();

    /*
     * Убираем служебные HTML-обёртки.
     */
    result = strip_html_wrapper(&result, "html");
    result = strip_html_wrapper(&result, "body");

    result
}

fn strip_html_wrapper(text: &str, tag: &str) -> String {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");

    let lower = text.to_lowercase();

    if lower.starts_with(&open) && lower.ends_with(&close) {
        let start = open.len();
        let end = text.len() - close.len();

        return text[start..end].trim().to_string();
    }

    text.to_string()
}

fn flush_paragraph(html: &mut String, lines: &mut Vec<&str>) {
    if lines.is_empty() {
        return;
    }

    let content = lines
        .iter()
        .map(|line| escape_html(line))
        .collect::<Vec<_>>()
        .join("<br>\n");

    html.push_str("<p>");
    html.push_str(&content);
    html.push_str("</p>\n");

    lines.clear();
}

pub fn escape_html(text: &str) -> String {
    let mut result = String::with_capacity(text.len());

    for ch in text.chars() {
        match ch {
            '&' => result.push_str("&amp;"),
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            '"' => result.push_str("&quot;"),
            '\'' => result.push_str("&#39;"),
            _ => result.push(ch),
        }
    }

    result
}

/* -------------------------------------------------------------------------- */
/* Normalization                                                              */
/* -------------------------------------------------------------------------- */

pub fn normalize_source_text(text: &str) -> String {
    text.replace("\r\n", "\n")
        .replace('\r', "\n")
        .trim_start_matches('\u{FEFF}')
        .to_string()
}

fn normalize_line_endings(text: &str) -> String {
    text.replace("\r\n", "\n")
        .replace('\r', "\n")
        .trim_start_matches('\u{FEFF}')
        .to_string()
}

fn normalize_heading_line(text: &str) -> String {
    text.trim()
        .trim_matches('\u{FEFF}')
        .trim_matches(|c: char| c == '\u{200B}' || c == '\u{00A0}')
        .trim()
        .to_string()
}

/* -------------------------------------------------------------------------- */
/* Encoding                                                                   */
/* -------------------------------------------------------------------------- */

pub fn decode_text(bytes: &[u8]) -> String {
    /*
     * UTF-8 — основной вариант.
     */
    if let Ok(text) = std::str::from_utf8(bytes) {
        return normalize_source_text(text);
    }

    /*
     * Windows-1251 — типичный вариант для русских TXT.
     */
    let (text, _, had_errors) = WINDOWS_1251.decode(bytes);

    if !had_errors {
        return normalize_source_text(&text);
    }

    /*
     * Последний fallback.
     */
    normalize_source_text(&String::from_utf8_lossy(bytes))
}

/* -------------------------------------------------------------------------- */
/* Book title                                                                  */
/* -------------------------------------------------------------------------- */

pub fn fallback_title(path: &std::path::Path, text: Option<String>) -> String {
    /*
     * Сначала пытаемся взять title из текста.
     */
    if let Some(text) = text {
        if let Some(title) = title_from_text(&text) {
            return title;
        }
    }

    /*
     * Потом имя файла.
     */
    if let Some(title) = title_from_filename(path) {
        return title;
    }

    "Untitled".to_string()
}

fn title_from_text(text: &str) -> Option<String> {
    for raw_line in text.lines().take(30) {
        let line = raw_line
            .trim()
            .trim_start_matches('\u{FEFF}')
            .trim();

        if line.is_empty() {
            continue;
        }

        /*
         * Не берём HTML-теги как название.
         */
        if line.starts_with('<') {
            continue;
        }

        if line.chars().count() > MAX_TITLE_LENGTH {
            continue;
        }

        let word_count = line.split_whitespace().count();

        if !(1..=10).contains(&word_count) {
            continue;
        }

        if looks_like_dialogue(line) {
            continue;
        }

        if looks_like_sentence(line) {
            continue;
        }

        let title = clean_title(line);

        if !title.is_empty() {
            return Some(title);
        }
    }

    None
}

fn title_from_filename(path: &std::path::Path) -> Option<String> {
    let stem = path.file_stem()?.to_str()?.trim();

    if stem.is_empty() {
        return None;
    }

    /*
     * avidreaders.ru__graf-monte-kristo
     *
     * ->
     *
     * avidreaders.ru graf monte kristo
     */
    let title = stem
        .replace("__", " — ")
        .replace(['_', '-'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    if title.is_empty() {
        None
    } else {
        Some(title)
    }
}

fn clean_title(text: &str) -> String {
    text.trim()
        .trim_matches(|c: char| {
            matches!(
                c,
                '#' | '*'
                    | '_'
                    | '"'
                    | '\''
                    | '“'
                    | '”'
                    | '«'
                    | '»'
                    | ' '
                    | '\t'
            )
        })
        .trim()
        .to_string()
}

/* -------------------------------------------------------------------------- */
/* HTML detection                                                             */
/* -------------------------------------------------------------------------- */

fn looks_like_html(text: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();

    let re = RE.get_or_init(|| {
        Regex::new(r"(?is)<(?:p|div|br|h[1-6]|body|html)(?:\s[^>]*)?>")
            .expect("valid html detection regex")
    });

    re.is_match(text)
}
