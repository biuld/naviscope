mod common;

use common::{offset_to_point, setup_java_test_graph};
use naviscope_plugin::{SymbolQueryService, SymbolResolveService};
use naviscope_java::JavaPlugin;

#[test]
fn test_goto_references_method() {
    let files = vec![
        ("A.java", "public class A { public void target() {} }"),
        ("B.java", "public class B { void m1(A a) { a.target(); } }"),
        ("C.java", "public class C { void m2(A a) { a.target(); } }"),
    ];
    let (index, trees) = setup_java_test_graph(files);
    let resolver = JavaPlugin::new().expect("Failed to create JavaPlugin");

    let a_content = &trees[0].1;
    let a_tree = &trees[0].2;

    // Resolve 'target' in A
    let usage_pos = a_content.find("target()").unwrap();
    let (line, col) = offset_to_point(a_content, usage_pos);
    let res = resolver
        .resolve_at(a_tree, a_content, line, col, &index)
        .expect("Should resolve target");
    let matches = resolver.find_matches(&index, &res);
    let target_fqn = matches[0];
    let target_idx = *index.fqn_map().get(&target_fqn).expect("Node not found");

    // Check for candidate files via DiscoveryEngine (Meso-scouting)
    let discovery = naviscope_core::features::discovery::DiscoveryEngine::new(
        &index,
        std::collections::HashMap::new(),
    );
    let candidate_files = discovery.scout_references(&[target_idx]);

    assert_eq!(candidate_files.len(), 3);
    let paths: Vec<String> = candidate_files
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();
    assert!(paths.contains(&"B.java".to_string()));
    assert!(paths.contains(&"C.java".to_string()));
}
