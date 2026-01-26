mod common;
use common::setup_java_test_graph;
use naviscope::index::CodeGraph;
use naviscope::model::graph::EdgeType;

/// Helper assertion: Verify that an edge of the specified type exists from source to target in the graph
fn assert_edge(graph: &CodeGraph, from_fqn: &str, to_fqn: &str, expected_type: EdgeType) {
    let from_idx = graph.fqn_map.get(from_fqn);
    let to_idx = graph.fqn_map.get(to_fqn);
    
    if from_idx.is_none() {
        println!("Available nodes:");
        for (id, _) in &graph.fqn_map {
            println!(" - {}", id);
        }
        panic!("Source node not found: {}", from_fqn);
    }
    if to_idx.is_none() {
        println!("Available nodes:");
        for (id, _) in &graph.fqn_map {
            println!(" - {}", id);
        }
        panic!("Target node not found: {}", to_fqn);
    }

    let edge_idx = graph.topology.find_edge(*from_idx.unwrap(), *to_idx.unwrap());
    
    if edge_idx.is_none() {
        println!("Graph nodes:");
        for (id, _) in &graph.fqn_map {
            println!(" - {}", id);
        }
        println!("Edges from {}:", from_fqn);
        let mut edges = graph.topology.neighbors_directed(*from_idx.unwrap(), petgraph::Direction::Outgoing).detach();
        while let Some((e_idx, target_idx)) = edges.next(&graph.topology) {
            let target_node = &graph.topology[target_idx];
            let edge = &graph.topology[e_idx];
            println!(" -> {} ({:?})", target_node.fqn(), edge.edge_type);
        }
        panic!("Edge not found between {} and {}", from_fqn, to_fqn);
    }
    
    let edge_weight = graph.topology.edge_weight(edge_idx.unwrap()).unwrap();
    assert_eq!(edge_weight.edge_type, expected_type, 
        "Edge type mismatch for {} -> {}. Expected {:?}, got {:?}", 
        from_fqn, to_fqn, expected_type, edge_weight.edge_type);
}

#[test]
fn test_edge_contains() {
    let files = vec![
        ("src/com/test/Container.java", r#"
            package com.test;
            public class Container {
                private int field;
                public void method() {}
            }
        "#),
    ];
    let (index, _) = setup_java_test_graph(files);

    // Class -> Field
    assert_edge(&index, "com.test.Container", "com.test.Container.field", EdgeType::Contains);
    // Class -> Method
    assert_edge(&index, "com.test.Container", "com.test.Container.method", EdgeType::Contains);
}

#[test]
fn test_edge_inherits_from() {
    let files = vec![
        ("src/Parent.java", "public class Parent {}"),
        ("src/Child.java", "public class Child extends Parent {}"),
    ];
    let (index, _) = setup_java_test_graph(files);

    assert_edge(&index, "Child", "Parent", EdgeType::InheritsFrom);
}

#[test]
fn test_edge_implements() {
    let files = vec![
        ("src/IAction.java", "public interface IAction {}"),
        ("src/ActionImpl.java", "public class ActionImpl implements IAction {}"),
    ];
    let (index, _) = setup_java_test_graph(files);

    assert_edge(&index, "ActionImpl", "IAction", EdgeType::Implements);
}

#[test]
fn test_edge_calls() {
    let files = vec![
        ("src/Service.java", r#"
            package com.test;
            public class Service {
                void run() { com.test.Service.helper(); }
                static void helper() {}
            }
        "#),
    ];
    let (index, _) = setup_java_test_graph(files);
    
    // Using FQN in call to ensure resolution works in batch mode
    assert_edge(&index, "com.test.Service.run", "com.test.Service.helper", EdgeType::Calls);
}

#[test]
fn test_edge_instantiates() {
    let files = vec![
        ("src/Factory.java", r#"
            public class Factory {
                void create() { new Product(); }
            }
        "#),
        ("src/Product.java", "public class Product {}"),
    ];
    let (index, _) = setup_java_test_graph(files);

    assert_edge(&index, "Factory.create", "Product", EdgeType::Instantiates);
}

#[test]
fn test_edge_typed_as() {
    let files = vec![
        ("src/User.java", r#"
            public class User {
                private String name;
                private Address address;
            }
        "#),
        ("src/Address.java", "public class Address {}"),
    ];
    let (index, _) = setup_java_test_graph(files);

    assert_edge(&index, "User.address", "Address", EdgeType::TypedAs);
}

#[test]
#[ignore]
fn test_edge_decorated_by() {
    let files = vec![
        ("src/Component.java", r#"
            @CustomAnnotation
            public class Component {}
        "#),
        ("src/CustomAnnotation.java", "public @interface CustomAnnotation {}"),
    ];
    let (index, _) = setup_java_test_graph(files);

    assert_edge(&index, "Component", "CustomAnnotation", EdgeType::DecoratedBy);
}
