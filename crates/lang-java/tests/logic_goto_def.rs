mod common;

use common::setup_java_test_graph;
use naviscope_core::features::CodeGraphLike;
use naviscope_core::ingest::parser::SymbolResolution;
use naviscope_core::ingest::resolver::SemanticResolver;
use naviscope_java::resolver::JavaResolver;

fn offset_to_point(content: &str, offset: usize) -> (usize, usize) {
    let pre_content = &content[..offset];
    let line = pre_content.lines().count().max(1) - 1;
    let last_newline = pre_content.rfind('\n').map(|p| p + 1).unwrap_or(0);
    let col = offset - last_newline;
    (line, col)
}

#[test]
fn test_goto_definition_local() {
    let files = vec![(
        "Test.java",
        "public class Test { void main() { int x = 1; int y = x + 1; } }",
    )];
    let (index, trees) = setup_java_test_graph(files);
    let resolver = JavaResolver::new();

    let content = &trees[0].1;
    let tree = &trees[0].2;

    // Position of 'x' in 'x + 1'
    let usage_pos = content.rfind("x + 1").unwrap();
    let (line, col) = offset_to_point(content, usage_pos);

    let res = resolver
        .resolve_at(tree, content, line, col, &index)
        .expect("Should resolve");

    if let SymbolResolution::Local(range, _) = res {
        // 'int x = 1' starts at index 35
        let def_pos = content.find("int x").unwrap() + 4;
        assert_eq!(range.start_col, def_pos);
    } else {
        panic!("Expected local resolution, got {:?}", res);
    }
}

#[test]
fn test_goto_definition_cross_file() {
    let files = vec![
        (
            "A.java",
            "package com; public class A { public void hello() {} }",
        ),
        (
            "B.java",
            "package com; public class B { void test() { A a = new A(); a.hello(); } }",
        ),
    ];
    let (index, trees) = setup_java_test_graph(files);
    let resolver = JavaResolver::new();

    let b_content = &trees[1].1;
    let b_tree = &trees[1].2;

    // 1. Resolve Class A
    let a_usage = b_content.find("A a").unwrap();
    let (line, col) = offset_to_point(b_content, a_usage);
    let res = resolver
        .resolve_at(b_tree, b_content, line, col, &index)
        .expect("Should resolve A");
    let matches = resolver.find_matches(&index, &res);
    assert!(!matches.is_empty());
    assert_eq!(index.render_fqn(&index.topology()[matches[0]], Some(&naviscope_java::naming::JavaNamingConvention)), "com.A");

    // 2. Resolve Method hello
    let hello_usage = b_content.find("hello()").unwrap();
    let (line, col) = offset_to_point(b_content, hello_usage);
    let res = resolver
        .resolve_at(b_tree, b_content, line, col, &index)
        .expect("Should resolve hello");
    let matches = resolver.find_matches(&index, &res);
    assert!(!matches.is_empty());
    assert_eq!(
        index.render_fqn(&index.topology()[matches[0]], Some(&naviscope_java::naming::JavaNamingConvention)),
        "com.A#hello"
    );
}

#[test]
fn test_goto_definition_shadowing() {
    let files = vec![(
        "Test.java",
        "public class Test { int x = 0; void m() { int x = 1; x = 2; } }",
    )];
    let (index, trees) = setup_java_test_graph(files);
    let resolver = JavaResolver::new();

    let content = &trees[0].1;
    let tree = &trees[0].2;

    // Position of 'x' in 'x = 2' (should be local x)
    let usage_pos = content.find("x = 2").unwrap();
    let (line, col) = offset_to_point(content, usage_pos);

    let res = resolver
        .resolve_at(tree, content, line, col, &index)
        .expect("Should resolve");

    if let SymbolResolution::Local(range, _) = res {
        let local_def = content.find("int x = 1").unwrap() + 4;
        assert_eq!(range.start_col, local_def);
    } else {
        panic!("Expected local resolution for shadowed x, got {:?}", res);
    }
}

#[test]
fn test_goto_definition_constructor() {
    let files = vec![
        ("A.java", "public class A { public A() {} }"),
        ("B.java", "public class B { A a = new A(); }"),
    ];
    let (index, trees) = setup_java_test_graph(files);
    let resolver = JavaResolver::new();

    let b_content = &trees[1].1;
    let b_tree = &trees[1].2;

    // Resolve 'A' in 'new A()'
    let usage_pos = b_content.find("new A()").unwrap() + 4;
    let (line, col) = offset_to_point(b_content, usage_pos);

    let res = resolver
        .resolve_at(b_tree, b_content, line, col, &index)
        .expect("Should resolve constructor");
    let matches = resolver.find_matches(&index, &res);
    assert!(!matches.is_empty());
    // In our model, constructor might be the class or the method depending on implementation
    assert!(
        index
            .render_fqn(&index.topology()[matches[0]], Some(&naviscope_java::naming::JavaNamingConvention))
            .contains("A")
    );
}

#[test]
fn test_goto_definition_static() {
    let files = vec![
        ("A.java", "public class A { public static int VAL = 1; }"),
        ("B.java", "public class B { int x = A.VAL; }"),
    ];
    let (index, trees) = setup_java_test_graph(files);
    let resolver = JavaResolver::new();

    let b_content = &trees[1].1;
    let b_tree = &trees[1].2;

    // Resolve 'VAL' in 'A.VAL'
    let usage_pos = b_content.find("VAL").unwrap();
    let (line, col) = offset_to_point(b_content, usage_pos);

    let res = resolver
        .resolve_at(b_tree, b_content, line, col, &index)
        .expect("Should resolve static field");
    let matches = resolver.find_matches(&index, &res);
    assert!(!matches.is_empty());
    assert_eq!(index.render_fqn(&index.topology()[matches[0]], Some(&naviscope_java::naming::JavaNamingConvention)), "A#VAL");
}
