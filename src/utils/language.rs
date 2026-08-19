use crate::{model::Chapter, utils::html::html_to_text};

const MIN_TEXT_LEN: usize = 20;
const SAMPLE_CHAPTERS: usize = 3;
const DEFAULT_LANGUAGE: &str = "en";

/// Определяет язык по первым нескольким главам.
pub fn detect_document_language(chapters: &[Chapter]) -> Option<String> {
    let text = chapters
        .iter()
        .take(3)
        .map(|c| html_to_text(&c.content.content()))
        .collect::<Vec<_>>()
        .join(" ");

    let sample: String = text.chars().take(10_000).collect();

    if sample.chars().count() < 50 {
        return None;
    }

    whatlang::detect(&sample)
        .map(|info| info.lang().code().to_string())
}
