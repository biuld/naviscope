use crate::parser::LspParser;
use crate::project::source::Language;
use std::path::PathBuf;
use std::sync::Arc;
use tower_lsp::lsp_types::Url;
use tree_sitter::Tree;

pub fn uri_to_path(uri: &Url) -> Option<PathBuf> {
    uri.to_file_path().ok()
}

/// Lightweight container for document state
pub struct Document {
    pub content: String,
    pub tree: Tree,
    pub parser: Arc<dyn LspParser>,
    pub language: Language,
}

impl Document {
    pub fn new(content: String, tree: Tree, parser: Arc<dyn LspParser>, language: Language) -> Self {
        Self {
            content,
            tree,
            parser,
            language,
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

pub fn find_node_at<'a>(tree: &'a Tree, content: &str, line: usize, utf16_col: usize) -> Option<tree_sitter::Node<'a>> {
    let root = tree.root_node();
    let byte_col = utf16_col_to_byte_col(content, line, utf16_col);
    let point = tree_sitter::Point::new(line, byte_col);
    let node = root.named_descendant_for_point_range(point, point)?;

    // If the node is not an identifier but we are at the end of one, try moving left by 1 byte
    if node.kind() != "identifier"
        && node.kind() != "type_identifier"
        && node.kind() != "scoped_identifier"
        && byte_col > 0
    {
        let prev_point = tree_sitter::Point::new(line, byte_col - 1);
        if let Some(prev_node) = root.named_descendant_for_point_range(prev_point, prev_point) {
            if prev_node.kind() == "identifier"
                || prev_node.kind() == "type_identifier"
                || prev_node.kind() == "scoped_identifier"
            {
                return Some(prev_node);
            }
        }
    }

    Some(node)
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

pub fn to_lsp_range(range: tree_sitter::Range, content: &str) -> tower_lsp::lsp_types::Range {
    tower_lsp::lsp_types::Range {
        start: to_lsp_position(range.start_point, content),
        end: to_lsp_position(range.end_point, content),
    }
}

pub fn to_lsp_position(point: tree_sitter::Point, content: &str) -> tower_lsp::lsp_types::Position {
    let line_idx = point.row;
    let byte_col = point.column;

    // Use split_terminator to handle all types of line endings and get the line efficiently
    let line_content = content.split_terminator('\n').nth(line_idx).unwrap_or("");
    let line_content = line_content.trim_end_matches('\r');
    
    let mut utf16_col = 0;
    let mut curr_byte = 0;

    for c in line_content.chars() {
        if curr_byte >= byte_col {
            break;
        }
        curr_byte += c.len_utf8();
        utf16_col += c.len_utf16();
    }

    tower_lsp::lsp_types::Position::new(line_idx as u32, utf16_col as u32)
}
