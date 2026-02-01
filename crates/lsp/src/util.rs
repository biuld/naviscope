use naviscope_api::models::Language;
use std::path::PathBuf;
use tower_lsp::lsp_types::Url;

pub fn uri_to_path(uri: &Url) -> Option<PathBuf> {
    uri.to_file_path().ok()
}

/// Lightweight container for document state
pub struct Document {
    pub content: String,
    pub language: Language,
    pub version: i32,
}

impl Document {
    pub fn new(content: String, language: Language, version: i32) -> Self {
        Self {
            content,
            language,
            version,
        }
    }
}

pub fn utf16_col_to_byte_col(content: &str, line: usize, utf16_col: usize) -> usize {
    let line_content = content.lines().nth(line).unwrap_or("");
    let mut curr_utf16 = 0;
    let mut curr_byte = 0;

    for c in line_content.chars() {
        if curr_utf16 >= utf16_col {
            break;
        }
        curr_utf16 += c.len_utf16();
        curr_byte += c.len_utf8();
    }
    curr_byte
}

pub fn get_word_at(path: &std::path::Path, line: usize, col: usize) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    get_word_from_content(&content, line, col)
}

pub fn get_word_from_content(content: &str, line: usize, col: usize) -> Option<String> {
    let line_content = content.lines().nth(line)?;

    // Find the start and end of the word (alphanumeric + _ + $)
    let is_ident = |c: char| c.is_alphanumeric() || c == '_' || c == '$';

    let start = line_content[..col.min(line_content.len())]
        .rfind(|c| !is_ident(c))
        .map(|i| i + 1)
        .unwrap_or(0);

    let end = line_content[col..]
        .find(|c| !is_ident(c))
        .map(|i| i + col)
        .unwrap_or(line_content.len());

    if start < end {
        Some(line_content[start..end].to_string())
    } else {
        None
    }
}
