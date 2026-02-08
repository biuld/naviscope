mod common;

use common::{offset_to_point, setup_java_test_graph};
use naviscope_core::features::CodeGraphLike;
use naviscope_core::features::discovery::DiscoveryEngine;
use naviscope_core::ingest::parser::SymbolResolution;
use naviscope_core::ingest::resolver::SemanticResolver;
use naviscope_java::lsp::JavaLspService;
use naviscope_java::resolver::JavaResolver;

#[test]
fn test_call_hierarchy_incoming() {
    let files = vec![(
        "Test.java",
        "public class Test { 
            void leaf() {} 
            void caller1() { leaf(); } 
            void caller2() { leaf(); }
            void root() { caller1(); caller2(); }
        }",
    )];
    let (index, trees) = setup_java_test_graph(files);
    let resolver = JavaResolver::new();

    let content = &trees[0].1;
    let tree = &trees[0].2;

    // Target: leaf()
    let leaf_pos = content.find("void leaf").unwrap() + 5;
    let (line, col) = offset_to_point(content, leaf_pos);
    let res = resolver
        .resolve_at(tree, content, line, col, &index)
        .expect("Should resolve leaf");
    let target_fqn = resolver.find_matches(&index, &res)[0];
    let target_idx = *index.fqn_map().get(&target_fqn).expect("Node not found");

    // Check callers using DiscoveryEngine
    let discovery = DiscoveryEngine::new(&index, std::collections::HashMap::new());
    let candidate_files = discovery.scout_references(&[target_idx]);

    let mut callers = Vec::new();
    let abs_path = std::env::current_dir().unwrap().join("Test.java");
    let uri = lsp_types::Url::from_file_path(&abs_path).unwrap();

    for path in candidate_files {
        let lsp_service = JavaLspService::new(std::sync::Arc::new(resolver.parser.clone()));
        let locations = discovery.scan_file(&lsp_service, &resolver, content, &res, &uri);
        for loc in locations {
            if let Some(container_idx) = index.find_container_node_at(
                &path,
                loc.range.start.line as usize,
                loc.range.start.character as usize,
            ) {
                // Skip if the occurrence is actually the definition of the target itself
                if let Some(name_range) = index.topology()[target_idx].name_range() {
                    if name_range.start_line == loc.range.start.line as usize
                        && name_range.start_col == loc.range.start.character as usize
                    {
                        continue;
                    }
                }
                let node = &index.topology()[container_idx];
                let fqn = index
                    .render_fqn(node, Some(&naviscope_java::naming::JavaNamingConvention))
                    .to_string();
                if !callers.contains(&fqn) {
                    callers.push(fqn);
                }
            }
        }
    }

    assert_eq!(callers.len(), 2);
    assert!(callers.contains(&"Test#caller1".to_string()));
    assert!(callers.contains(&"Test#caller2".to_string()));
}

#[test]
fn test_call_hierarchy_outgoing() {
    let files = vec![(
        "Test.java",
        "public class Test { 
            void root() { step1(); step2(); } 
            void step1() {} 
            void step2() {}
        }",
    )];
    let (index, trees) = setup_java_test_graph(files);
    let resolver = JavaResolver::new();

    let content = &trees[0].1;
    let tree = &trees[0].2;

    // Target: root()
    let root_pos = content.find("void root").unwrap() + 5;
    let (line, col) = offset_to_point(content, root_pos);
    let res = resolver
        .resolve_at(tree, content, line, col, &index)
        .expect("Should resolve root");
    let target_fqn = resolver.find_matches(&index, &res)[0];
    let target_idx = *index.fqn_map().get(&target_fqn).expect("Node not found");

    // Check callees using manual walk (similar to outgoing_calls in LSP)
    let container_range = index.topology()[target_idx].range().unwrap();
    let mut callees = Vec::new();

    let mut stack = vec![tree.root_node()];
    while let Some(n) = stack.pop() {
        let r = n.range();
        if r.start_point.row > container_range.end_line
            || r.end_point.row < container_range.start_line
        {
            continue;
        }

        if n.kind() == "identifier" {
            if let Some(out_res) = resolver.resolve_at(
                tree,
                content,
                r.start_point.row,
                r.start_point.column,
                &index,
            ) {
                let target_fqn = match out_res {
                    SymbolResolution::Global(fqn) => Some(fqn),
                    SymbolResolution::Precise(fqn, _) => Some(fqn),
                    _ => None,
                };

                if let Some(fqn) = target_fqn {
                    if !callees.contains(&fqn) && fqn != "Test#root" {
                        callees.push(fqn);
                    }
                }
            }
        }

        let mut cursor = n.walk();
        for child in n.children(&mut cursor) {
            stack.push(child);
        }
    }

    assert_eq!(callees.len(), 2);
    assert!(callees.contains(&"Test#step1".to_string()));
    assert!(callees.contains(&"Test#step2".to_string()));
}

#[test]
fn test_call_hierarchy_recursion() {
    let files = vec![(
        "Test.java",
        "public class Test { 
            void rec() { rec(); } 
        }",
    )];
    let (index, trees) = setup_java_test_graph(files);
    let resolver = JavaResolver::new();

    let content = &trees[0].1;
    let tree = &trees[0].2;

    let pos = content.find("void rec").unwrap() + 5;
    let (line, col) = offset_to_point(content, pos);
    let res = resolver
        .resolve_at(tree, content, line, col, &index)
        .unwrap();
    let target_fqn = resolver.find_matches(&index, &res)[0];
    let idx = *index.fqn_map().get(&target_fqn).expect("Node not found");

    // Incoming should contain itself
    let discovery = DiscoveryEngine::new(&index, std::collections::HashMap::new());
    let mut callers = Vec::new();
    let abs_path = std::env::current_dir().unwrap().join("Test.java");
    let uri = lsp_types::Url::from_file_path(&abs_path).unwrap();

    let lsp_service = JavaLspService::new(std::sync::Arc::new(resolver.parser.clone()));
    let locations = discovery.scan_file(&lsp_service, &resolver, content, &res, &uri);
    for loc in locations {
        if let Some(c_idx) = index.find_container_node_at(
            &std::path::PathBuf::from("Test.java"),
            loc.range.start.line as usize,
            loc.range.start.character as usize,
        ) {
            // Skip if the occurrence is actually the definition of the target itself
            if let Some(name_range) = index.topology()[idx].name_range() {
                if name_range.start_line == loc.range.start.line as usize
                    && name_range.start_col == loc.range.start.character as usize
                {
                    continue;
                }
            }
            let node = &index.topology()[c_idx];
            let fqn = index
                .render_fqn(
                    node,
                    Some(&naviscope_java::naming::JavaNamingConvention::default()),
                )
                .to_string();
            if !callers.contains(&fqn) {
                callers.push(fqn);
            }
        }
    }

    assert!(callers.contains(&"Test#rec".to_string()));
}
