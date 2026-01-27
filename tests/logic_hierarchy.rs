mod common;

use naviscope::resolver::lang::java::JavaResolver;
use naviscope::resolver::SemanticResolver;
use naviscope::model::graph::EdgeType;
use common::setup_java_test_graph;
use petgraph::Direction;

fn offset_to_point(content: &str, offset: usize) -> (usize, usize) {
    let pre_content = &content[..offset];
    let line = pre_content.lines().count().max(1) - 1;
    let last_newline = pre_content.rfind('\n').map(|p| p + 1).unwrap_or(0);
    let col = offset - last_newline;
    (line, col)
}

#[test]
fn test_call_hierarchy_incoming() {
    let files = vec![
        ("Test.java", "public class Test { 
            void leaf() {} 
            void caller1() { leaf(); } 
            void caller2() { leaf(); }
            void root() { caller1(); caller2(); }
        }"),
    ];
    let (index, trees) = setup_java_test_graph(files);
    let resolver = JavaResolver::new();

    let content = &trees[0].1;
    let tree = &trees[0].2;

    // Target: leaf()
    let leaf_pos = content.find("void leaf").unwrap() + 5;
    let (line, col) = offset_to_point(content, leaf_pos);
    let res = resolver.resolve_at(tree, content, line, col, &index).expect("Should resolve leaf");
    let target_idx = resolver.find_matches(&index, &res)[0];

    // Check callers
    let mut callers = Vec::new();
    let mut incoming = index.topology.neighbors_directed(target_idx, Direction::Incoming).detach();
    while let Some((edge_idx, neighbor_idx)) = incoming.next(&index.topology) {
        if index.topology[edge_idx].edge_type == EdgeType::Calls {
            callers.push(index.topology[neighbor_idx].fqn().to_string());
        }
    }
    
    assert_eq!(callers.len(), 2);
    assert!(callers.contains(&"Test.caller1".to_string()));
    assert!(callers.contains(&"Test.caller2".to_string()));
}

#[test]
fn test_call_hierarchy_outgoing() {
    let files = vec![
        ("Test.java", "public class Test { 
            void root() { step1(); step2(); } 
            void step1() {} 
            void step2() {}
        }"),
    ];
    let (index, trees) = setup_java_test_graph(files);
    let resolver = JavaResolver::new();

    let content = &trees[0].1;
    let tree = &trees[0].2;

    // Target: root()
    let root_pos = content.find("void root").unwrap() + 5;
    let (line, col) = offset_to_point(content, root_pos);
    let res = resolver.resolve_at(tree, content, line, col, &index).expect("Should resolve root");
    let target_idx = resolver.find_matches(&index, &res)[0];

    // Check callees
    let mut callees = Vec::new();
    let mut outgoing = index.topology.neighbors_directed(target_idx, Direction::Outgoing).detach();
    while let Some((edge_idx, neighbor_idx)) = outgoing.next(&index.topology) {
        if index.topology[edge_idx].edge_type == EdgeType::Calls {
            callees.push(index.topology[neighbor_idx].fqn().to_string());
        }
    }
    
    assert_eq!(callees.len(), 2);
    assert!(callees.contains(&"Test.step1".to_string()));
    assert!(callees.contains(&"Test.step2".to_string()));
}

#[test]
fn test_call_hierarchy_recursion() {
    let files = vec![
        ("Test.java", "public class Test { 
            void rec() { rec(); } 
        }"),
    ];
    let (index, trees) = setup_java_test_graph(files);
    let resolver = JavaResolver::new();

    let content = &trees[0].1;
    let tree = &trees[0].2;

    let pos = content.find("void rec").unwrap() + 5;
    let (line, col) = offset_to_point(content, pos);
    let res = resolver.resolve_at(tree, content, line, col, &index).unwrap();
    let idx = resolver.find_matches(&index, &res)[0];

    // Incoming should contain itself
    let callers: Vec<_> = index.topology.neighbors_directed(idx, Direction::Incoming)
        .filter(|&n| index.topology[index.topology.find_edge(n, idx).unwrap()].edge_type == EdgeType::Calls)
        .map(|n| index.topology[n].fqn().to_string())
        .collect();
    
    assert!(callers.contains(&"Test.rec".to_string()));
}
