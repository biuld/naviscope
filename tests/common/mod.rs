use naviscope::index::CodeGraph;
use naviscope::model::graph::GraphOp;
use naviscope::parser::java::JavaParser;
use naviscope::parser::IndexParser;
use naviscope::resolver::lang::java::JavaResolver;
use naviscope::project::scanner::{ParsedFile, ParsedContent};
use naviscope::project::source::SourceFile;
use naviscope::resolver::ProjectContext;
use std::path::PathBuf;
use tree_sitter::Parser;

pub fn setup_java_test_graph(files: Vec<(&str, &str)>) -> (CodeGraph, Vec<(PathBuf, String, tree_sitter::Tree)>) {
    let mut index = CodeGraph::new();
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

    // Apply Node operations first
    for op in &all_ops {
        if let GraphOp::AddNode { .. } = op {
            apply_op_to_graph(&mut index, op.clone());
        }
    }

    // Apply Edge operations next
    for op in &all_ops {
        if let GraphOp::AddEdge { .. } = op {
            apply_op_to_graph(&mut index, op.clone());
        }
    }

    (index, parsed_files)
}

pub fn apply_op_to_graph(index: &mut CodeGraph, op: GraphOp) {
    match op {
        GraphOp::AddNode { id, data } => {
            let path = data.file_path().cloned();
            let idx = index.get_or_create_node(&id, data);
            if let Some(p) = path {
                index.path_to_nodes.entry(p).or_default().push(idx);
            }
        }
        GraphOp::AddEdge { from_id, to_id, edge } => {
            let from_idx = index.fqn_map.get(&from_id).cloned();
            let to_idx = index.fqn_map.get(&to_id).cloned();
            if let (Some(s_idx), Some(t_idx)) = (from_idx, to_idx) {
                index.topology.add_edge(s_idx, t_idx, edge);
            }
        }
        _ => {}
    }
}
