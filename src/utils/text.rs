use std::sync::OnceLock;

use encoding_rs::WINDOWS_1251;
use regex::Regex;

use crate::model::{Chapter, ChapterContent};

const MIN_CHAPTER_CHARS: usize = 800;
const TARGET_CHAPTER_CHARS: usize = 12_000;
const MAX_CHAPTER_CHARS: usize = 18_000;
const MIN_TITLE_LENGTH: usize = 2;
const MAX_TITLE_LENGTH: usize = 120;

#[derive(Debug, Clone)]
struct TextBlock {
    text: String,
}

#[derive(Debug, Clone)]
struct ChapterBoundary {
    block_index: usize,
    title: String,
}

/// Разделяет обычный текстовый файл на главы.
///
/// Алгоритм:
/// 1. Нормализация текста.
/// 2. Поиск явных заголовков.
/// 3. Поиск вероятных заголовков по эвристике.
/// 4. Если заголовков нет — fallback-разбиение по абзацам.
/// 5. Если текст представляет собой одну длинную простыню —
///    разбиение по целевому количеству символов.
pub fn split_into_chapters(text: &str) -> Vec<Chapter> {
    let text = normalize_source_text(text);

    if text.trim().is_empty() {
        return Vec::new();
    }

    let blocks = split_into_blocks(&text);

    if blocks.is_empty() {
        return Vec::new();
    }

    // Сначала ищем реальные заголовки.
    let boundaries = detect_chapter_boundaries(&blocks);

    if !boundaries.is_empty() {
        let chapters = build_chapters_from_boundaries(&blocks, &boundaries);

        if !chapters.is_empty() {
            return chapters;
        }
    }

    // Если заголовки не обнаружены —
    // пробуем разделить книгу эвристически.
    split_without_headings(&blocks)
}

fn normalize_source_text(text: &str) -> String {
    text.trim_start_matches('\u{FEFF}')
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace(['\u{00A0}', '\t'], " ")
}

fn split_into_blocks(text: &str) -> Vec<TextBlock> {
    let mut blocks = Vec::new();
    let mut current_lines = Vec::new();

    for raw_line in text.lines() {
        let line = raw_line.trim();

        if line.is_empty() {
            if !current_lines.is_empty() {
                let block = current_lines.join(" ");

                if !block.trim().is_empty() {
                    blocks.push(TextBlock {
                        text: normalize_block_text(&block),
                    });
                }

                current_lines.clear();
            }

            continue;
        }

        current_lines.push(line.to_string());
    }

    if !current_lines.is_empty() {
        let block = current_lines.join(" ");

        if !block.trim().is_empty() {
            blocks.push(TextBlock {
                text: normalize_block_text(&block),
            });
        }
    }

    blocks
}

fn normalize_block_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn detect_chapter_boundaries(blocks: &[TextBlock]) -> Vec<ChapterBoundary> {
    let mut result = Vec::new();
    let mut previous_title: Option<String> = None;

    for (index, block) in blocks.iter().enumerate() {
        let Some(title) = detect_title(block, blocks, index) else {
            continue;
        };

        // Не допускаем один и тот же заголовок подряд.
        if previous_title.as_deref() == Some(title.as_str()) {
            continue;
        }

        previous_title = Some(title.clone());

        result.push(ChapterBoundary {
            block_index: index,
            title,
        });
    }

    // Первый найденный заголовок должен быть достаточно рано.
    // Иначе это может быть просто строка внутри обычного текста.
    if let Some(first) = result.first() {
        if first.block_index > blocks.len() / 3 {
            return Vec::new();
        }
    }

    result
}

fn detect_title(block: &TextBlock, blocks: &[TextBlock], index: usize) -> Option<String> {
    let text = block.text.trim();

    if text.len() < MIN_TITLE_LENGTH || text.len() > MAX_TITLE_LENGTH {
        return None;
    }

    // Слишком длинная строка почти наверняка не заголовок.
    let word_count = text.split_whitespace().count();

    if word_count > 15 {
        return None;
    }

    // Явные названия глав имеют максимальный приоритет.
    if is_explicit_chapter_title(text) {
        return Some(clean_title(text));
    }

    let mut score = 0;

    // Короткая строка.
    if text.len() <= 80 {
        score += 1;
    }

    // Очень короткая строка характерна для title.
    if text.len() <= 50 {
        score += 1;
    }

    // Нет финальной пунктуации.
    if !ends_with_sentence_punctuation(text) {
        score += 1;
    }

    // Заголовок часто не заканчивается точкой.
    if !text.ends_with('.') {
        score += 1;
    }

    // Title Case.
    if is_title_case(text) {
        score += 2;
    }

    // Все буквы заглавные.
    if is_all_caps(text) {
        score += 2;
    }

    // Содержит номер в начале.
    if starts_with_number(text) {
        score += 3;
    }

    // Римская цифра в начале.
    if starts_with_roman_number(text) {
        score += 3;
    }

    // Строка находится между пустыми блоками —
    // в нашем представлении каждый block уже отделён пустой строкой,
    // поэтому дополнительно смотрим на соседние блоки.
    if index > 0 && index + 1 < blocks.len() {
        let prev_len = blocks[index - 1].text.len();
        let next_len = blocks[index + 1].text.len();

        // Заголовок обычно короткий, а соседние блоки длиннее.
        if text.len() * 3 < prev_len.max(next_len) {
            score += 2;
        }

        if prev_len > 200 {
            score += 1;
        }

        if next_len > 200 {
            score += 1;
        }
    }

    // Слишком похожа на обычное предложение.
    if looks_like_sentence(text) {
        score -= 3;
    }

    if score >= 5 {
        Some(clean_title(text))
    } else {
        None
    }
}

fn is_explicit_chapter_title(text: &str) -> bool {
    static EXPLICIT: OnceLock<Vec<Regex>> = OnceLock::new();

    let patterns = EXPLICIT.get_or_init(|| {
        vec![
            Regex::new(
                r"(?i)^(chapter|chap\.?|глава|гл\.?)\s+([0-9]{1,4}|[ivxlcdm]+|[a-zа-яё]+)\b.*$",
            )
            .unwrap(),
            Regex::new(r"(?i)^(part|часть)\s+([0-9]{1,4}|[ivxlcdm]+|[a-zа-яё]+)\b.*$").unwrap(),
            Regex::new(r"(?i)^(book|книга)\s+([0-9]{1,4}|[ivxlcdm]+|[a-zа-яё]+)\b.*$").unwrap(),
            Regex::new(r"(?i)^(section|section\.|секция|раздел)\s+([0-9]{1,4}|[ivxlcdm]+)\b.*$")
                .unwrap(),
            Regex::new(r"(?i)^(prologue|epilogue|introduction|foreword|afterword|appendix)$")
                .unwrap(),
            Regex::new(r"(?i)^(пролог|эпилог|введение|предисловие|послесловие|приложение)$")
                .unwrap(),
            Regex::new(
                r"(?i)^(пролог|эпилог|введение|предисловие|послесловие|приложение)\s*[:\-—]?.*$",
            )
            .unwrap(),
        ]
    });

    patterns.iter().any(|pattern| pattern.is_match(text))
}

fn starts_with_number(text: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();

    let re =
        RE.get_or_init(|| Regex::new(r"^\s*(?:chapter\s+)?\d{1,4}(?:\s*[\.\-:)]|\s+|$)").unwrap());

    re.is_match(text)
}

fn starts_with_roman_number(text: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();

    let re = RE.get_or_init(|| {
        Regex::new(r"(?i)^\s*(?:chapter|глава)?\s*[ivxlcdm]{1,8}(?:\s*[\.\-:)]|\s+|$)").unwrap()
    });

    re.is_match(text)
}

fn is_title_case(text: &str) -> bool {
    let words: Vec<&str> = text.split_whitespace().collect();

    if words.is_empty() {
        return false;
    }

    let mut meaningful_words = 0;
    let mut title_case_words = 0;

    for word in words {
        let clean = word.trim_matches(|c: char| !c.is_alphabetic());

        if clean.is_empty() {
            continue;
        }

        meaningful_words += 1;

        if let Some(first) = clean.chars().next() {
            if first.is_uppercase() {
                title_case_words += 1;
            }
        }
    }

    meaningful_words > 0 && title_case_words as f32 / meaningful_words as f32 >= 0.6
}

fn is_all_caps(text: &str) -> bool {
    let letters: Vec<char> = text.chars().filter(|c| c.is_alphabetic()).collect();

    if letters.len() < 3 {
        return false;
    }

    letters.iter().all(|c| !c.is_lowercase())
}

fn looks_like_sentence(text: &str) -> bool {
    let words = text.split_whitespace().count();

    if words < 4 {
        return false;
    }

    let sentence_end =
        text.ends_with('.') || text.ends_with('!') || text.ends_with('?') || text.ends_with('…');

    if sentence_end {
        return true;
    }

    // Есть типичный субъект + сказуемое.
    // Это грубая эвристика, зато хорошо отсекает
    // большинство обычных предложений.
    let lower = text.to_lowercase();

    lower.contains(" is ")
        || lower.contains(" are ")
        || lower.contains(" was ")
        || lower.contains(" were ")
        || lower.contains(" has ")
        || lower.contains(" have ")
        || lower.contains(" had ")
        || lower.contains(" и ")
        || lower.contains(" это ")
        || lower.contains(" был ")
        || lower.contains(" была ")
        || lower.contains(" были ")
        || lower.contains(" есть ")
}

fn ends_with_sentence_punctuation(text: &str) -> bool {
    text.ends_with('.')
        || text.ends_with('!')
        || text.ends_with('?')
        || text.ends_with('…')
        || text.ends_with(';')
}

fn clean_title(text: &str) -> String {
    text.trim()
        .trim_matches(|c: char| matches!(c, '#' | '*' | '_' | '"' | '\'' | '“' | '”' | '«' | '»'))
        .trim()
        .to_string()
}

fn build_chapters_from_boundaries(
    blocks: &[TextBlock],
    boundaries: &[ChapterBoundary],
) -> Vec<Chapter> {
    let mut chapters = Vec::new();

    for (position, boundary) in boundaries.iter().enumerate() {
        let start = boundary.block_index + 1;

        let end = boundaries
            .get(position + 1)
            .map(|next| next.block_index)
            .unwrap_or(blocks.len());

        // Заголовок должен иметь хотя бы немного текста после себя.
        if start >= end {
            continue;
        }

        let content_blocks = &blocks[start..end];

        let html = blocks_to_html(content_blocks);

        if html.trim().is_empty() {
            continue;
        }

        chapters.push(Chapter {
            title: Some(boundary.title.clone()),
            content: ChapterContent::Html(html),
        });
    }

    // Если перед первой главой был текст (например, посвящение),
    // добавляем его как отдельную главу без title.
    if let Some(first_boundary) = boundaries.first() {
        if first_boundary.block_index > 0 {
            let preface = &blocks[..first_boundary.block_index];

            if !preface.is_empty() {
                chapters.insert(
                    0,
                    Chapter {
                        title: None,
                        content: ChapterContent::Html(blocks_to_html(preface)),
                    },
                );
            }
        }
    }

    chapters
}

fn split_without_headings(blocks: &[TextBlock]) -> Vec<Chapter> {
    if blocks.len() <= 1 {
        return vec![Chapter {
            title: None,
            content: ChapterContent::Html(blocks_to_html(blocks)),
        }];
    }

    // ------------------------------------------------------------
    // Fallback №1:
    // если текст состоит из хорошо разделённых крупных блоков,
    // считаем каждый блок потенциальным разделом.
    // ------------------------------------------------------------

    let meaningful_blocks: Vec<&TextBlock> = blocks
        .iter()
        .filter(|block| block.text.chars().count() >= 100)
        .collect();

    if meaningful_blocks.len() >= 3 {
        let total_chars: usize = blocks.iter().map(|block| block.text.chars().count()).sum();

        let average = total_chars / blocks.len().max(1);

        if average > 250 {
            return split_by_balanced_blocks(blocks);
        }
    }

    // ------------------------------------------------------------
    // Fallback №2:
    // длинный текст без абзацной структуры.
    // Разрезаем примерно по TARGET_CHAPTER_CHARS,
    // но только между блоками.
    // ------------------------------------------------------------

    split_by_target_size(blocks)
}

fn split_by_balanced_blocks(blocks: &[TextBlock]) -> Vec<Chapter> {
    let total_chars: usize = blocks.iter().map(|block| block.text.chars().count()).sum();

    // Для небольшого текста одна глава.
    if total_chars < MIN_CHAPTER_CHARS * 2 {
        return vec![Chapter {
            title: None,
            content: ChapterContent::Html(blocks_to_html(blocks)),
        }];
    }

    // Стараемся получить 3–15k символов на часть.
    let target = TARGET_CHAPTER_CHARS.min(total_chars.max(MIN_CHAPTER_CHARS));

    let mut chapters = Vec::new();
    let mut current = Vec::new();
    let mut current_chars = 0;

    for block in blocks {
        let block_chars = block.text.chars().count();

        if !current.is_empty()
            && current_chars >= MIN_CHAPTER_CHARS
            && current_chars + block_chars > target
        {
            chapters.push(Chapter {
                title: None,
                content: ChapterContent::Html(blocks_to_html(&current)),
            });

            current.clear();
            current_chars = 0;
        }

        current.push(block.clone());
        current_chars += block_chars;
    }

    if !current.is_empty() {
        chapters.push(Chapter {
            title: None,
            content: ChapterContent::Html(blocks_to_html(&current)),
        });
    }

    chapters
}

fn split_by_target_size(blocks: &[TextBlock]) -> Vec<Chapter> {
    let total_chars: usize = blocks.iter().map(|block| block.text.chars().count()).sum();

    if total_chars <= TARGET_CHAPTER_CHARS {
        return vec![Chapter {
            title: None,
            content: ChapterContent::Html(blocks_to_html(blocks)),
        }];
    }

    let mut chapters = Vec::new();

    let mut current = Vec::new();
    let mut current_chars = 0;

    for block in blocks {
        let block_chars = block.text.chars().count();

        let should_split = !current.is_empty()
            && current_chars >= MIN_CHAPTER_CHARS
            && (current_chars + block_chars > TARGET_CHAPTER_CHARS
                || current_chars >= MAX_CHAPTER_CHARS);

        if should_split {
            let order = chapters.len() + 1;

            chapters.push(Chapter {
                title: Some(format!("Part {}", order)),
                content: ChapterContent::Html(blocks_to_html(&current)),
            });

            current.clear();
            current_chars = 0;
        }

        current.push(block.clone());
        current_chars += block_chars;
    }

    if !current.is_empty() {
        let title = if chapters.is_empty() {
            None
        } else {
            Some(format!("Part {}", chapters.len() + 1))
        };

        chapters.push(Chapter {
            title,
            content: ChapterContent::Html(blocks_to_html(&current)),
        });
    }

    chapters
}

fn blocks_to_html(blocks: &[TextBlock]) -> String {
    let mut html = String::new();

    for block in blocks {
        let text = escape_html(&block.text);

        if text.is_empty() {
            continue;
        }

        html.push_str("<p>");
        html.push_str(&text);
        html.push_str("</p>\n");
    }

    html
}

/// Converts plain text into safe, readable HTML.
///
/// Rules:
/// - HTML entities are escaped.
/// - Consecutive non-empty lines are grouped into paragraphs.
/// - Single newlines inside a paragraph become `<br>`.
/// - Empty lines separate paragraphs.
/// - Whitespace is normalized.
/// - UTF-8 text is preserved as-is.
pub fn text_to_html(text: &str) -> String {
    let text = normalize_line_endings(text);

    if text.trim().is_empty() {
        return String::new();
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

fn normalize_line_endings(text: &str) -> String {
    text.replace("\r\n", "\n")
        .replace('\r', "\n")
        .trim_start_matches('\u{FEFF}')
        .to_string()
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

use std::path::Path;

pub fn fallback_title(path: &Path, text: Option<String>) -> String {
    // 1. Пытаемся найти заголовок в самом тексте.
    if let Some(text) = text {
        if let Some(title) = title_from_text(&text) {
            return title;
        }
    }

    // 2. Используем имя файла.
    if let Some(title) = title_from_filename(path) {
        return title;
    }

    // 3. Последний fallback.
    "Untitled".to_string()
}

fn title_from_text(text: &str) -> Option<String> {
    for line in text.lines() {
        let line = line.trim().trim_start_matches('\u{FEFF}').trim();

        if line.is_empty() {
            continue;
        }

        // Слишком длинная строка почти наверняка является текстом,
        // а не названием.
        if line.chars().count() > 120 {
            return None;
        }

        // Заголовок обычно содержит небольшое количество слов.
        let word_count = line.split_whitespace().count();

        if !(1..=15).contains(&word_count) {
            return None;
        }

        // Обычное длинное предложение не считаем заголовком.
        if looks_like_sentence(line) {
            return None;
        }

        let title = clean_title(line);

        if !title.is_empty() {
            return Some(title);
        }
    }

    None
}

fn title_from_filename(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_str()?.trim();

    if stem.is_empty() {
        return None;
    }

    let title = stem
        .replace(['_', '-'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    if title.is_empty() { None } else { Some(title) }
}

pub fn decode_text(bytes: &[u8]) -> String {
    if let Ok(text) = std::str::from_utf8(bytes) {
        return normalize_text(text);
    }

    let (text, _, had_errors) = WINDOWS_1251.decode(bytes);

    if !had_errors {
        return normalize_text(&text);
    }

    let text = String::from_utf8_lossy(bytes);

    normalize_text(&text)
}

pub fn normalize_text(text: &str) -> String {
    text.replace("\r\n", "\n")
        .replace('\r', "\n")
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

pub fn normalize_whitespace(text: &str) -> String {
    text.replace("\r\n", "\n")
        .replace('\r', "\n")
        .split("\n\n")
        .map(|paragraph| {
            paragraph
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .filter(|paragraph| !paragraph.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}
