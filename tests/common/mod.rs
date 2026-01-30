use naviscope::engine::{CodeGraph, CodeGraphBuilder};
use naviscope::model::graph::GraphOp;
use naviscope::parser::IndexParser;
use naviscope::parser::java::JavaParser;
use naviscope::project::scanner::{ParsedContent, ParsedFile};
use naviscope::project::source::SourceFile;
use naviscope::resolver::ProjectContext;
use naviscope::resolver::lang::java::JavaResolver;
use std::path::PathBuf;
use tree_sitter::Parser;

pub fn setup_java_test_graph(
    files: Vec<(&str, &str)>,
) -> (CodeGraph, Vec<(PathBuf, String, tree_sitter::Tree)>) {
    let mut builder = CodeGraphBuilder::new();
    let mut parsed_files = Vec::new();
    let java_parser = JavaParser::new().unwrap();
    let mut ts_parser = Parser::new();
    ts_parser.set_language(&java_parser.language).unwrap();

    // Phase 1: Parse all files to get entities and build the graph
    let mut all_parsed_files = Vec::new();
    for (path_str, content) in files {
        let path = PathBuf::from(path_str);
        let res = java_parser.parse_file(content, Some(&path)).unwrap();
        let source_file = SourceFile::new(path.clone(), 0, 0);
        let parsed_file = ParsedFile {
            file: source_file,
            content: ParsedContent::Java(res),
        };
        all_parsed_files.push((parsed_file, content.to_string()));
    }

    // Phase 2: Resolve (using JavaResolver's LangResolver implementation)
    let resolver = JavaResolver::new();
    let context = ProjectContext::new(); // Empty context for simple tests

    let mut all_ops = Vec::new();

    for (pf, content) in all_parsed_files {
        let tree = ts_parser.parse(&content, None).unwrap();

        // Use LangResolver to get graph operations
        use naviscope::resolver::LangResolver;
        let unit = resolver.resolve(&pf, &context).unwrap();
        all_ops.extend(unit.ops);

        parsed_files.push((pf.file.path.clone(), content.to_string(), tree));
    }

    // Apply operations in two passes to ensure nodes exist before edges
    // Pass 1: Nodes
    for op in &all_ops {
        if let GraphOp::AddNode { .. } = op {
            builder.apply_op(op.clone()).unwrap();
        }
    }

    // Pass 2: Edges and others
    for op in &all_ops {
        if !matches!(op, GraphOp::AddNode { .. }) {
            builder.apply_op(op.clone()).unwrap();
        }
    }

    (builder.build(), parsed_files)
}
