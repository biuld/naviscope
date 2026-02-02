use super::JavaParser;
use naviscope_core::model::{EdgeType, Range};
use std::collections::HashMap;
use tree_sitter::{Node, QueryCapture, StreamingIterator, Tree};

mod entities;
mod metadata;
mod relations;

/// The native semantic model of a Java source file.
pub struct JavaFileModel<'a> {
    pub package: Option<String>,
    pub imports: Vec<String>,
    pub entities: Vec<JavaEntity<'a>>,
    pub relations: Vec<JavaRelation>,
    pub identifiers: Vec<String>,
}

pub struct JavaEntity<'a> {
    pub element: crate::model::JavaIndexMetadata,
    pub node: Node<'a>,
    pub fqn: naviscope_api::models::symbol::NodeId,
    pub name: String,
}

pub struct JavaRelation {
    pub source_id: naviscope_api::models::symbol::NodeId,
    pub target_id: naviscope_api::models::symbol::NodeId,
    pub rel_type: EdgeType,
    pub range: Option<Range>,
}

impl JavaParser {
    /// Deeply analyzes a Java tree and produces a native JavaFileModel.
    pub(crate) fn analyze<'a>(&self, tree: &'a Tree, source: &'a str) -> JavaFileModel<'a> {
        let (package, imports) = self.extract_package_and_imports(tree, source);
        let all_matches = self.collect_matches(tree, source);

        let mut entities = Vec::new();
        let mut relations = Vec::new();
        let mut entities_map = HashMap::<naviscope_api::models::symbol::NodeId, usize>::new();

        // Stage 1: Identify all named entities (Classes, Methods, Fields)
        self.identify_entities(
            &all_matches,
            source,
            &package,
            &mut entities,
            &mut relations,
            &mut entities_map,
        );

        // Stage 2: Enrich identified entities with metadata (Annotations, Inheritance, Types)
        self.enrich_metadata(
            &all_matches,
            source,
            &package,
            &mut entities,
            &mut relations,
            &entities_map,
        );

        // Stage 3: Collect Reference Index (Identifiers)
        let identifiers = self.collect_identifiers(tree, source);

        JavaFileModel {
            package,
            imports,
            entities,
            relations,
            identifiers,
        }
    }

    pub(crate) fn collect_matches<'a>(
        &self,
        tree: &'a Tree,
        source: &'a str,
    ) -> Vec<Vec<QueryCapture<'a>>> {
        let mut cursor = tree_sitter::QueryCursor::new();
        let mut matches =
            cursor.matches(&self.definition_query, tree.root_node(), source.as_bytes());
        let mut all_matches = Vec::new();
        while let Some(mat) = matches.next() {
            all_matches.push(mat.captures.to_vec());
        }
        all_matches
    }
    pub(crate) fn collect_identifiers(&self, tree: &Tree, source: &str) -> Vec<String> {
        let mut identifiers = std::collections::HashSet::new();
        let mut stack = vec![tree.root_node()];

        while let Some(node) = stack.pop() {
            let kind = node.kind();
            if kind == "identifier" || kind == "type_identifier" {
                if let Ok(text) = node.utf8_text(source.as_bytes()) {
                    identifiers.insert(text.to_string());
                }
            }

            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                stack.push(child);
            }
        }

        identifiers.into_iter().collect()
    }
}
