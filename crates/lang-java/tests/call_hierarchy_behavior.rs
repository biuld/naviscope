mod common;

use common::{offset_to_point, setup_java_test_graph};
use naviscope_core::features::CodeGraphLike;
use naviscope_core::features::discovery::DiscoveryEngine;
use naviscope_api::models::SymbolResolution;
use naviscope_java::JavaPlugin;
use naviscope_plugin::{SymbolQueryService, SymbolResolveService};

#[test]
fn given_leaf_method_when_find_incoming_callers_then_returns_direct_callers_only() {
    let files = vec![(
        "Test.java",
        "public class Test { void leaf() {} void c1() { leaf(); } void c2() { leaf(); } }",
    )];

    let (index, trees) = setup_java_test_graph(files);
    let resolver = JavaPlugin::new().expect("Failed to create JavaPlugin");

    let content = &trees[0].1;
    let tree = &trees[0].2;
    let leaf_pos = content.find("void leaf").expect("find leaf") + 5;
    let (line, col) = offset_to_point(content, leaf_pos);
    let resolution = resolver
        .resolve_at(tree, content, line, col, &index)
        .expect("resolve leaf symbol");

    let target = resolver.find_matches(&index, &resolution);
    let target_idx = *index.fqn_map().get(&target[0]).expect("target node exists");

    let discovery = DiscoveryEngine::new(&index, std::collections::HashMap::new());
    let abs_path = std::env::current_dir().expect("cwd").join("Test.java");
    let uri: lsp_types::Uri = format!("file://{}", abs_path.display())
        .parse()
        .expect("valid uri");

    let semantic = JavaPlugin::new().expect("failed to create java plugin");
    let locations = discovery.scan_file(&semantic, content, &resolution, &uri);

    let mut callers = Vec::new();
    for loc in locations {
        if let Some(name_range) = index.topology()[target_idx].name_range()
            && name_range.start_line == loc.range.start.line as usize
            && name_range.start_col == loc.range.start.character as usize
        {
            continue;
        }

        if let Some(container_idx) = index.find_container_node_at(
            &std::path::PathBuf::from("Test.java"),
            loc.range.start.line as usize,
            loc.range.start.character as usize,
        ) {
            let node = &index.topology()[container_idx];
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

    callers.sort();
    assert_eq!(callers, vec!["Test#c1", "Test#c2"]);
}

#[test]
fn given_root_method_when_find_outgoing_calls_then_returns_direct_callees() {
    let files = vec![(
        "Test.java",
        "public class Test { void root() { step1(); step2(); } void step1() {} void step2() {} }",
    )];

    let (index, trees) = setup_java_test_graph(files);
    let resolver = JavaPlugin::new().expect("Failed to create JavaPlugin");

    let content = &trees[0].1;
    let tree = &trees[0].2;
    let root_pos = content.find("void root").expect("find root") + 5;
    let (line, col) = offset_to_point(content, root_pos);
    let resolution = resolver
        .resolve_at(tree, content, line, col, &index)
        .expect("resolve root");

    let target_fqn = resolver.find_matches(&index, &resolution)[0];
    let target_idx = *index.fqn_map().get(&target_fqn).expect("target node exists");
    let container_range = index.topology()[target_idx].range().expect("container range");

    let mut callees = Vec::new();
    let mut stack = vec![tree.root_node()];
    while let Some(node) = stack.pop() {
        let range = node.range();
        if range.start_point.row > container_range.end_line
            || range.end_point.row < container_range.start_line
        {
            continue;
        }

        if node.kind() == "identifier"
            && let Some(out_res) = resolver.resolve_at(
                tree,
                content,
                range.start_point.row,
                range.start_point.column,
                &index,
            )
        {
            let maybe_fqn = match out_res {
                SymbolResolution::Global(fqn) => Some(fqn),
                SymbolResolution::Precise(fqn, _) => Some(fqn),
                _ => None,
            };

            if let Some(fqn) = maybe_fqn
                && fqn.contains('#')
                && fqn != "Test#root"
                && !callees.contains(&fqn)
            {
                callees.push(fqn);
            }
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }

    callees.sort();
    assert_eq!(callees, vec!["Test#step1", "Test#step2"]);
}

#[test]
fn given_recursive_method_when_find_incoming_callers_then_includes_self_call() {
    let files = vec![("Test.java", "public class Test { void rec() { rec(); } }")];

    let (index, trees) = setup_java_test_graph(files);
    let resolver = JavaPlugin::new().expect("Failed to create JavaPlugin");

    let content = &trees[0].1;
    let tree = &trees[0].2;
    let rec_pos = content.find("void rec").expect("find rec") + 5;
    let (line, col) = offset_to_point(content, rec_pos);
    let resolution = resolver
        .resolve_at(tree, content, line, col, &index)
        .expect("resolve rec");

    let target_fqn = resolver.find_matches(&index, &resolution)[0];
    let target_idx = *index.fqn_map().get(&target_fqn).expect("target node exists");

    let discovery = DiscoveryEngine::new(&index, std::collections::HashMap::new());
    let abs_path = std::env::current_dir().expect("cwd").join("Test.java");
    let uri: lsp_types::Uri = format!("file://{}", abs_path.display())
        .parse()
        .expect("valid uri");

    let semantic = JavaPlugin::new().expect("failed to create java plugin");
    let locations = discovery.scan_file(&semantic, content, &resolution, &uri);

    let mut callers = Vec::new();
    for loc in locations {
        if let Some(name_range) = index.topology()[target_idx].name_range()
            && name_range.start_line == loc.range.start.line as usize
            && name_range.start_col == loc.range.start.character as usize
        {
            continue;
        }

        if let Some(container_idx) = index.find_container_node_at(
            &std::path::PathBuf::from("Test.java"),
            loc.range.start.line as usize,
            loc.range.start.character as usize,
        ) {
            let node = &index.topology()[container_idx];
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
