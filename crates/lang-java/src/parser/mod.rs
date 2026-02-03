type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
use std::sync::Arc;
use tree_sitter::{Query, StreamingIterator, Tree};

mod ast;
mod constants;
mod index;
mod lsp;
mod naming;
mod scope;
mod types;

use crate::queries::java_definitions::JavaIndices;
use crate::queries::java_occurrences::OccurrenceIndices;

pub struct JavaParser {
    pub language: tree_sitter::Language,
    pub(crate) definition_query: Arc<Query>,
    pub(crate) indices: JavaIndices,
    pub(crate) occurrence_query: Arc<Query>,
    pub(crate) occurrence_indices: OccurrenceIndices,
}

impl Clone for JavaParser {
    fn clone(&self) -> Self {
        Self {
            language: self.language.clone(),
            definition_query: Arc::clone(&self.definition_query),
            indices: self.indices.clone(),
            occurrence_query: Arc::clone(&self.occurrence_query),
            occurrence_indices: self.occurrence_indices.clone(),
        }
    }
}

impl JavaParser {
    pub fn new() -> Result<Self> {
        let language: tree_sitter::Language = tree_sitter_java::LANGUAGE.into();

        let definition_query = naviscope_plugin::utils::load_query(
            &language,
            crate::queries::java_definitions::JAVA_DEFINITIONS_SCM,
        )?;
        let indices = JavaIndices::new(&definition_query)?;

        let occurrence_query = naviscope_plugin::utils::load_query(
            &language,
            crate::queries::java_occurrences::JAVA_OCCURRENCES_SCM,
        )?;
        let occurrence_indices = OccurrenceIndices::new(&occurrence_query)?;

        Ok(Self {
            language,
            definition_query: Arc::new(definition_query),
            indices,
            occurrence_query: Arc::new(occurrence_query),
            occurrence_indices,
        })
    }

    pub fn extract_package_and_imports(
        &self,
        tree: &Tree,
        source: &str,
    ) -> (Option<String>, Vec<String>) {
        let mut package = None;
        let mut imports = Vec::new();
        let mut cursor = tree_sitter::QueryCursor::new();
        let mut matches =
            cursor.matches(&self.definition_query, tree.root_node(), source.as_bytes());
        while let Some(mat) = matches.next() {
            if let Some(cap) = mat.captures.iter().find(|c| c.index == self.indices.pkg) {
                package = cap
                    .node
                    .utf8_text(source.as_bytes())
                    .ok()
                    .map(|s: &str| s.to_string());
            } else if let Some(cap) = mat
                .captures
                .iter()
                .find(|c| c.index == self.indices.import_name)
            {
                if let Ok(imp) = cap.node.utf8_text(source.as_bytes()) {
                    let imp_str: &str = imp;
                    imports.push(imp_str.to_string());
                }
            }
        }
        (package, imports)
    }
}
