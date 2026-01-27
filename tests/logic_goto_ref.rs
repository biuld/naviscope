mod common;

use naviscope::resolver::lang::java::JavaResolver;
use naviscope::resolver::SemanticResolver;
use naviscope::model::graph::EdgeType;
use common::setup_java_test_graph;
use petgraph::Direction;

fn offset_to_point(content: &str, offset: usize) -> (usize, usize) {
    let pre_content = &content[..offset];
    let line: usize = pre_content.lines().count().max(1) - 1;
    let last_newline = pre_content.rfind('\n').map(|p| p + 1).unwrap_or(0);
    let col = offset - last_newline;
    (line, col)
}

#[test]
fn test_goto_references_method() {
    let files = vec![
        ("A.java", "public class A { public void target() {} }"),
        ("B.java", "public class B { void m1(A a) { a.target(); } }"),
        ("C.java", "public class C { void m2(A a) { a.target(); } }"),
    ];
    let (index, trees) = setup_java_test_graph(files);
    let resolver = JavaResolver::new();

    let a_content = &trees[0].1;
    let a_tree = &trees[0].2;

    // Resolve 'target' in A
    let usage_pos = a_content.find("target()").unwrap();
    let (line, col) = offset_to_point(a_content, usage_pos);
    let res = resolver.resolve_at(a_tree, a_content, line, col, &index).expect("Should resolve target");
    let matches = resolver.find_matches(&index, &res);
    let target_idx = matches[0];

    // Check incoming 'Calls' edges
    let mut callers = Vec::new();
    let mut incoming = index.topology.neighbors_directed(target_idx, Direction::Incoming).detach();
    while let Some((edge_idx, neighbor_idx)) = incoming.next(&index.topology) {
        let edge = &index.topology[edge_idx];
        if edge.edge_type == EdgeType::Calls {
            callers.push(index.topology[neighbor_idx].fqn().to_string());
        }
    }

    assert_eq!(callers.len(), 2);
    assert!(callers.contains(&"B.m1".to_string()));
    assert!(callers.contains(&"C.m2".to_string()));
}
