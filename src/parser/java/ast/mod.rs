use super::JavaParser;
use crate::model::graph::{EdgeType, Range};
use crate::model::lang::java::JavaElement;
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
}

pub struct JavaEntity<'a> {
    pub element: JavaElement,
    pub node: Node<'a>,
}

pub struct JavaRelation {
    pub source_fqn: String,
    pub target_name: String,
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
        let mut entities_map = HashMap::new();

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

        // Stage 3: Resolve semantic relations (Method Calls, Instantiations)
        self.resolve_relations(&all_matches, source, &package, &mut relations);

        JavaFileModel {
            package,
            imports,
            entities,
            relations,
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
}
