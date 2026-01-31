mod common;

use common::setup_java_test_graph;
use naviscope_core::resolver::SemanticResolver;
use naviscope_java::resolver::JavaResolver;

fn offset_to_point(content: &str, offset: usize) -> (usize, usize) {
    let pre_content = &content[..offset];
    let line = pre_content.lines().count().max(1) - 1;
    let last_newline = pre_content.rfind('\n').map(|p| p + 1).unwrap_or(0);
    let col = offset - last_newline;
    (line, col)
}

#[test]
fn test_goto_implementation_interface() {
    let files = vec![
        ("IBase.java", "public interface IBase { void act(); }"),
        (
            "ImplA.java",
            "public class ImplA implements IBase { public void act() {} }",
        ),
        (
            "ImplB.java",
            "public class ImplB implements IBase { public void act() {} }",
        ),
    ];
    let (index, trees) = setup_java_test_graph(files);
    let resolver = JavaResolver::new();

    let base_content = &trees[0].1;
    let base_tree = &trees[0].2;

    // Resolve 'IBase'
    let usage_pos = base_content.find("IBase").unwrap();
    let (line, col) = offset_to_point(base_content, usage_pos);
    let res = resolver
        .resolve_at(base_tree, base_content, line, col, &index)
        .expect("Should resolve IBase");

    let impls = resolver.find_implementations(&index, &res);
    assert_eq!(impls.len(), 2);

    let fqns: Vec<_> = impls
        .iter()
        .map(|&i| index.topology()[i].fqn().to_string())
        .collect();
    assert!(fqns.contains(&"ImplA".to_string()));
    assert!(fqns.contains(&"ImplB".to_string()));
}

#[test]
fn test_goto_implementation_method() {
    let files = vec![
        ("IBase.java", "public interface IBase { void act(); }"),
        (
            "Impl.java",
            "public class Impl implements IBase { public void act() {} }",
        ),
    ];
    let (index, trees) = setup_java_test_graph(files);
    let resolver = JavaResolver::new();

    let base_content = &trees[0].1;
    let base_tree = &trees[0].2;

    // Resolve 'act' in IBase
    let usage_pos = base_content.find("act()").unwrap();
    let (line, col) = offset_to_point(base_content, usage_pos);
    let res = resolver
        .resolve_at(base_tree, base_content, line, col, &index)
        .expect("Should resolve act");

    let impls = resolver.find_implementations(&index, &res);
    assert_eq!(impls.len(), 1);
    assert_eq!(index.topology()[impls[0]].fqn(), "Impl.act");
}
