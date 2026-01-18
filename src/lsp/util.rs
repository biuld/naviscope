use crate::index::{Naviscope, NaviscopeIndex};
use std::path::PathBuf;
use tower_lsp::lsp_types::Url;

pub fn uri_to_path(uri: &Url) -> Option<PathBuf> {
    uri.to_file_path().ok()
}

pub fn get_word_at(path: &std::path::Path, line: usize, col: usize) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
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

pub struct IndexAccess<'a> {
    pub naviscope: &'a Naviscope,
}

impl<'a> IndexAccess<'a> {
    pub fn index(&self) -> &NaviscopeIndex {
        self.naviscope.index()
    }
}
