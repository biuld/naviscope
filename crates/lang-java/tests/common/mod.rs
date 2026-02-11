use naviscope_api::models::Language;
use naviscope_core::ingest::builder::CodeGraphBuilder;
use naviscope_java::JavaPlugin;
use naviscope_java::parser::JavaParser;
use naviscope_plugin::{
    GraphOp, ParsedContent, ParsedFile, ProjectContext, SourceFile, SourceIndexCap,
};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Once;
use tree_sitter::Parser;

#[allow(dead_code)]
pub fn setup_java_test_graph(
    files: Vec<(&str, &str)>,
) -> (
    naviscope_core::model::CodeGraph,
    Vec<(PathBuf, String, tree_sitter::Tree)>,
) {
    let mut builder = CodeGraphBuilder::new();
    builder.naming_conventions.insert(
        Language::JAVA,
        Arc::new(naviscope_java::naming::JavaNamingConvention::default()),
    );
    let mut parsed_files = Vec::new();
    let java_parser = JavaParser::new().unwrap();
    let mut ts_parser = Parser::new();
    ts_parser
        .set_language(&java_parser.language.clone().into())
        .unwrap();

    // Phase 1: Parse all files to get entities and build the graph
    let mut all_parsed_files = Vec::new();
    for (path_str, content) in files {
        let path = PathBuf::from(path_str);
        let res = java_parser.parse_file(content, Some(&path)).unwrap();
        let source_file = SourceFile::new(path.clone(), 0, 0);
        let parsed_file = ParsedFile {
            file: source_file,
            content: ParsedContent::Language(res),
        };
        all_parsed_files.push((parsed_file, content.to_string()));
    }

    // Phase 2: Resolve (using JavaResolver source-index implementation)
    let resolver = JavaPlugin::new().expect("Failed to create JavaPlugin");
    let context = ProjectContext::new(); // Uses default V2 context

    let mut all_ops = Vec::new();

    for (pf, content) in all_parsed_files {
        let tree = ts_parser.parse(&content, None).unwrap();

        // Use JavaResolver to get resolved unit
        let unit = resolver.compile_source(&pf, &context).unwrap();

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

#[allow(dead_code)]
pub fn offset_to_point(content: &str, offset: usize) -> (usize, usize) {
    let pre_content = &content[..offset];
    let line = pre_content.lines().count().max(1) - 1;
    let last_newline = pre_content.rfind('\n').map(|p| p + 1).unwrap_or(0);
    let col = offset - last_newline;
    (line, col)
}

#[allow(dead_code)]
pub async fn setup_java_engine(
    temp_dir: &std::path::Path,
    files: Vec<(&str, &str)>,
) -> naviscope_core::facade::EngineHandle {
    ensure_test_index_dir();
    use naviscope_core::runtime::orchestrator::NaviscopeEngine as CoreEngine;
    let java_caps = naviscope_java::java_caps().expect("Failed to create Java caps");
    let engine = CoreEngine::builder(temp_dir.to_path_buf())
        .with_language_caps(java_caps)
        .build();

    // Create files
    for (path_str, content) in &files {
        let path = temp_dir.join(path_str);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, content).unwrap();
    }

    // Index files
    let paths: Vec<_> = files.iter().map(|(p, _)| temp_dir.join(p)).collect();
    engine.update_files(paths).await.unwrap();

    naviscope_core::facade::EngineHandle::from_engine(Arc::new(engine))
}

fn ensure_test_index_dir() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let dir = std::env::temp_dir().join("naviscope_test_index_dir");
        std::fs::create_dir_all(&dir).unwrap();
        unsafe {
            std::env::set_var("NAVISCOPE_INDEX_DIR", dir);
        }
    });
}
