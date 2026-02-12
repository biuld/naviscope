mod common;

use common::{offset_to_point, setup_java_test_graph};
use naviscope_api::models::symbol::Range;
use naviscope_core::features::discovery::DiscoveryEngine;
use naviscope_java::JavaPlugin;
use naviscope_plugin::{LspSyntaxService, SymbolQueryService, SymbolResolveService};
use std::collections::BTreeSet;

fn starts_set(ranges: &[Range]) -> BTreeSet<(usize, usize)> {
    ranges.iter().map(|r| (r.start_line, r.start_col)).collect()
}

fn find_ranges_for_path(
    resolver: &JavaPlugin,
    index: &naviscope_core::model::CodeGraph,
    trees: &[(std::path::PathBuf, String, tree_sitter::Tree)],
    resolution: &naviscope_api::models::SymbolResolution,
    path: &str,
) -> Vec<Range> {
    let (.., source, tree) = trees
        .iter()
        .find(|(p, _, _)| p.to_string_lossy() == path)
        .expect("path exists in trees");
    resolver.find_occurrences(source, tree, resolution, Some(index))
}

#[test]
fn given_same_method_name_different_owner_when_find_references_then_only_target_owner_hits() {
    let files = vec![
        ("A.java", "public class A { void target() {} }"),
        ("B.java", "public class B { void target() {} }"),
        (
            "Use.java",
            "public class Use { void run(A a, B b) { a.target(); b.target(); } }",
        ),
    ];

    let (index, trees) = setup_java_test_graph(files);
    let resolver = JavaPlugin::new().expect("Failed to create JavaPlugin");

    let a_content = &trees[0].1;
    let a_tree = &trees[0].2;
    let target_pos = a_content.find("target()").expect("find A#target");
    let (line, col) = offset_to_point(a_content, target_pos);

    let resolution = resolver
        .resolve_at(a_tree, a_content, line, col, &index)
        .expect("resolve A#target");

    let a_ranges = find_ranges_for_path(&resolver, &index, &trees, &resolution, "A.java");
    let b_ranges = find_ranges_for_path(&resolver, &index, &trees, &resolution, "B.java");
    let use_ranges = find_ranges_for_path(&resolver, &index, &trees, &resolution, "Use.java");

    let a_decl_pos = a_content.find("target()").expect("find A declaration");
    let a_decl_start = offset_to_point(a_content, a_decl_pos);

    let use_content = &trees[2].1;
    let use_call_pos = use_content.find("a.target()").expect("find a.target()") + 2;
    let use_call_start = offset_to_point(use_content, use_call_pos);

    assert_eq!(starts_set(&a_ranges), BTreeSet::from([a_decl_start]));
    assert!(b_ranges.is_empty(), "B.java should not be polluted by same-name method");
    assert_eq!(starts_set(&use_ranges), BTreeSet::from([use_call_start]));
}

#[test]
fn given_local_shadowing_when_find_references_then_only_same_binding_hits() {
    let files = vec![(
        "Test.java",
        "public class Test { int x = 0; void run() { int x = 1; x = 2; } }",
    )];

    let (index, trees) = setup_java_test_graph(files);
    let resolver = JavaPlugin::new().expect("Failed to create JavaPlugin");

    let content = &trees[0].1;
    let tree = &trees[0].2;
    let usage_pos = content.find("x = 2").expect("find local usage");
    let (line, col) = offset_to_point(content, usage_pos);

    let resolution = resolver
        .resolve_at(tree, content, line, col, &index)
        .expect("resolve local x");

    let ranges = find_ranges_for_path(&resolver, &index, &trees, &resolution, "Test.java");
    let starts = starts_set(&ranges);
    let decl_pos = content.find("int x = 1").expect("find local declaration") + 4;
    let assign_pos = content.find("x = 2").expect("find local assign");
    let expected = BTreeSet::from([
        offset_to_point(content, decl_pos),
        offset_to_point(content, assign_pos),
    ]);
    assert_eq!(starts, expected, "local x should only match declaration + assignment");

    let matches = resolver.find_matches(&index, &resolution);
    assert!(
        matches.is_empty(),
        "local symbol should not map to global graph nodes"
    );
}

#[test]
fn given_method_symbol_when_scouting_references_then_candidate_files_cover_call_sites() {
    let files = vec![
        ("A.java", "public class A { public void target() {} }"),
        ("B.java", "public class B { void m1(A a) { a.target(); } }"),
        ("C.java", "public class C { void m2(A a) { a.target(); } }"),
    ];

    let (index, trees) = setup_java_test_graph(files);
    let resolver = JavaPlugin::new().expect("Failed to create JavaPlugin");

    let a_content = &trees[0].1;
    let a_tree = &trees[0].2;
    let usage_pos = a_content.find("target()").expect("find declaration");
    let (line, col) = offset_to_point(a_content, usage_pos);

    let resolution = resolver
        .resolve_at(a_tree, a_content, line, col, &index)
        .expect("resolve method");
    let matches = resolver.find_matches(&index, &resolution);
    let target_fqn = matches[0];
    let target_idx = *index.fqn_map().get(&target_fqn).expect("node exists");

    let discovery = DiscoveryEngine::new(&index, std::collections::HashMap::new());
    let candidate_files = discovery.scout_references(&[target_idx]);

    let paths: Vec<String> = candidate_files
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();

    assert_eq!(paths.len(), 3);
    assert!(paths.contains(&"A.java".to_string()));
    assert!(paths.contains(&"B.java".to_string()));
    assert!(paths.contains(&"C.java".to_string()));
}

#[test]
fn given_overloaded_methods_same_owner_when_find_references_then_matches_all_name_level_overloads() {
    let files = vec![
        (
            "A.java",
            "public class A { void target(int n) {} void target(String s) {} }",
        ),
        (
            "Use.java",
            "public class Use { void run(A a) { a.target(1); a.target(\"x\"); } }",
        ),
    ];

    let (index, trees) = setup_java_test_graph(files);
    let resolver = JavaPlugin::new().expect("Failed to create JavaPlugin");

    let a_content = &trees[0].1;
    let a_tree = &trees[0].2;
    let target_pos = a_content.find("target(int").expect("find overloaded declaration");
    let (line, col) = offset_to_point(a_content, target_pos);

    let resolution = resolver
        .resolve_at(a_tree, a_content, line, col, &index)
        .expect("resolve overloaded method");

    let a_ranges = find_ranges_for_path(&resolver, &index, &trees, &resolution, "A.java");
    let use_ranges = find_ranges_for_path(&resolver, &index, &trees, &resolution, "Use.java");

    let a_decl_int = a_content.find("target(int n)").expect("find target(int)");
    let a_decl_str = a_content
        .find("target(String s)")
        .expect("find target(String)");
    let use_content = &trees[1].1;
    let use_call_int = use_content.find("a.target(1)").expect("find call int") + 2;
    let use_call_str = use_content.find("a.target(\"x\")").expect("find call str") + 2;

    assert_eq!(
        starts_set(&a_ranges),
        BTreeSet::from([
            offset_to_point(a_content, a_decl_int),
            offset_to_point(a_content, a_decl_str),
        ])
    );
    assert_eq!(
        starts_set(&use_ranges),
        BTreeSet::from([
            offset_to_point(use_content, use_call_int),
            offset_to_point(use_content, use_call_str),
        ])
    );
}

#[test]
fn given_same_class_different_arity_overloads_when_find_references_then_collects_decls_and_calls() {
    let files = vec![(
        "A.java",
        "public class A { void target() { target(1); target(1, 2); } void target(int a) {} void target(int a, int b) {} }",
    )];

    let (index, trees) = setup_java_test_graph(files);
    let resolver = JavaPlugin::new().expect("Failed to create JavaPlugin");

    let content = &trees[0].1;
    let tree = &trees[0].2;
    let pos = content.find("target()").expect("find zero-arity declaration");
    let (line, col) = offset_to_point(content, pos);

    let resolution = resolver
        .resolve_at(tree, content, line, col, &index)
        .expect("resolve overloaded member");

    let ranges = find_ranges_for_path(&resolver, &index, &trees, &resolution, "A.java");
    let starts = starts_set(&ranges);

    let decl_zero = content.find("target() {").expect("find target()");
    let call_one = content.find("target(1);").expect("find target(1)");
    let call_two = content.find("target(1, 2);").expect("find target(1,2)");
    let decl_one = content.find("target(int a)").expect("find target(int)");
    let decl_two = content
        .find("target(int a, int b)")
        .expect("find target(int,int)");

    let expected = BTreeSet::from([
        offset_to_point(content, decl_zero),
        offset_to_point(content, call_one),
        offset_to_point(content, call_two),
        offset_to_point(content, decl_one),
        offset_to_point(content, decl_two),
    ]);
    assert_eq!(starts, expected);
}
