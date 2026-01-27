use crate::error::Result;
use tree_sitter::{Query, Tree, StreamingIterator};
use std::sync::Arc;

mod constants;
mod lsp;
mod index;
mod ast;
mod types;
mod naming;
mod scope;

unsafe extern "C" {
    fn tree_sitter_java() -> tree_sitter::Language;
}

use crate::parser::queries::java_definitions::JavaIndices;

pub struct JavaParser {
    pub language: tree_sitter::Language,
    pub(crate) definition_query: Arc<Query>,
    pub(crate) indices: JavaIndices,
}

impl Clone for JavaParser {
    fn clone(&self) -> Self {
        Self {
            language: self.language.clone(),
            definition_query: Arc::clone(&self.definition_query),
            indices: self.indices.clone(),
        }
    }
}

impl JavaParser {
    pub fn new() -> Result<Self> {
        let language = unsafe { tree_sitter_java() };
        let definition_query = crate::parser::utils::load_query(
            &language,
            include_str!("../queries/java_definitions.scm"),
        )?;
        let indices = JavaIndices::new(&definition_query)?;

        Ok(Self {
            language,
            definition_query: Arc::new(definition_query),
            indices,
        })
    }

    pub fn extract_package_and_imports(&self, tree: &Tree, source: &str) -> (Option<String>, Vec<String>) {
        let mut package = None;
        let mut imports = Vec::new();
        let mut cursor = tree_sitter::QueryCursor::new();
        let mut matches = cursor.matches(&self.definition_query, tree.root_node(), source.as_bytes());
        while let Some(mat) = matches.next() {
            if let Some(cap) = mat.captures.iter().find(|c| c.index == self.indices.pkg) {
                package = cap.node.utf8_text(source.as_bytes()).ok().map(|s: &str| s.to_string());
            } else if let Some(cap) = mat.captures.iter().find(|c| c.index == self.indices.import_name) {
                if let Ok(imp) = cap.node.utf8_text(source.as_bytes()) {
                    let imp_str: &str = imp;
                    imports.push(imp_str.to_string());
                }
            }
        }
        (package, imports)
    }
}
