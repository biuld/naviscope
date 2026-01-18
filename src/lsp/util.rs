use crate::index::{Naviscope, NaviscopeIndex};
use std::path::PathBuf;
use tower_lsp::lsp_types::Url;

pub fn uri_to_path(uri: &Url) -> Option<PathBuf> {
    uri.to_file_path().ok()
}

pub struct IndexAccess<'a> {
    pub naviscope: &'a Naviscope,
}

impl<'a> IndexAccess<'a> {
    pub fn index(&self) -> &NaviscopeIndex {
        self.naviscope.index()
    }
}
