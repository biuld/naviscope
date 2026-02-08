mod common;

use common::setup_java_test_graph;
use naviscope_core::features::discovery::DiscoveryEngine;
use naviscope_core::model::EdgeType;

/// Capability 1: Structural Indexing
/// The graph MUST represent the project structure (Package -> Class -> Method/Field)
#[test]
fn cap_structural_nesting() {
    let files = vec![(
        "com/example/MyClass.java",
        "package com.example; public class MyClass { int field; void method() {} }",
    )];
    let (index, _) = setup_java_test_graph(files);

    // Assert FQNs exist
    // Note: JavaResolver uses clear package names now for FQN compatibility
    println!("Graph nodes:");
    for idx in index.topology().node_indices() {
        let node = &index.topology()[idx];
        use naviscope_plugin::NamingConvention;
        println!(
            " - {:?}",
            naviscope_plugin::StandardNamingConvention.render_fqn(node.id, index.fqns())
        );
    }

    assert!(index.find_node("com.example").is_some());
    assert!(index.find_node("com.example.MyClass").is_some());
    assert!(index.find_node("com.example.MyClass#field").is_some());
    assert!(index.find_node("com.example.MyClass#method").is_some());

    // Assert nesting via 'Contains' edges
    let class_idx = index.find_node("com.example.MyClass").unwrap();
    let pkg_idx = index.find_node("com.example").unwrap();

    assert!(index.topology().contains_edge(pkg_idx, class_idx));

    let field_idx = index.find_node("com.example.MyClass#field").unwrap();
    let method_idx = index.find_node("com.example.MyClass#method").unwrap();
    assert!(index.topology().contains_edge(class_idx, field_idx));
    assert!(index.topology().contains_edge(class_idx, method_idx));
}

/// Capability 2: Inheritance & Implementation
/// The graph MUST track class hierarchies
#[test]
fn cap_inheritance_tracking() {
    let files = vec![
        ("Base.java", "public interface Base {}"),
        ("Impl.java", "public class Impl implements Base {}"),
    ];
    let (index, _) = setup_java_test_graph(files);

    let base_idx = index.find_node("Base").unwrap();
    let impl_idx = index.find_node("Impl").unwrap();

    println!("Base ID: {:?}", index.topology()[base_idx].id);
    println!("Impl ID: {:?}", index.topology()[impl_idx].id);

    println!("Edges from Impl:");
    let mut neighbors = index
        .topology()
        .neighbors_directed(impl_idx, petgraph::Direction::Outgoing)
        .detach();
    while let Some((e_idx, target_idx)) = neighbors.next(&index.topology()) {
        let edge = &index.topology()[e_idx];
        let target = &index.topology()[target_idx];
        use naviscope_plugin::NamingConvention;
        println!(
            " -> {:?} connection {:?} (Target ID: {:?})",
            edge.edge_type,
            naviscope_plugin::StandardNamingConvention.render_fqn(target.id, index.fqns()),
            target.id
        );
    }

    let has_implements = index
        .topology()
        .edges_connecting(impl_idx, base_idx)
        .any(|e| e.weight().edge_type == EdgeType::Implements);

    assert!(
        has_implements,
        "Graph should have Implements edge from Impl to Base"
    );
}

/// Capability 3: Cross-File Type Resolution (TypedAs)
/// The graph MUST resolve types across files during indexing to link members to their types
#[test]
fn cap_cross_file_typing() {
    let files = vec![
        (
            "com/lib/TypeA.java",
            "package com.lib; public class TypeA {}",
        ),
        (
            "com/app/Main.java",
            "package com.app; import com.lib.TypeA; public class Main { TypeA field; }",
        ),
    ];
    let (index, _) = setup_java_test_graph(files);

    let field_idx = index.find_node("com.app.Main#field").unwrap();
    let type_a_idx = index.find_node("com.lib.TypeA").unwrap();

    let has_typed_as = index
        .topology()
        .edges_connecting(field_idx, type_a_idx)
        .any(|e| e.weight().edge_type == EdgeType::TypedAs);

    assert!(
        has_typed_as,
        "Field 'Main.field' should be linked to 'TypeA' via TypedAs edge"
    );
}

/// Capability 4: Direct Instantiation Tracking
/// The graph uses reference_index to discover instantiation references
#[test]
fn cap_instantiation_tracking() {
    let files = vec![
        ("A.java", "public class A {}"),
        ("B.java", "public class B { void m() { A a = new A(); } }"),
    ];
    let (index, _) = setup_java_test_graph(files);

    let a_idx = index.find_node("A").unwrap();

    // Check DiscoveryEngine "Scouting" (uses Reference Index)
    let discovery = DiscoveryEngine::new(&index, std::collections::HashMap::new());
    let candidate_files = discovery.scout_references(&[a_idx]);
    assert!(
        candidate_files.contains(&std::path::PathBuf::from("B.java")),
        "DiscoveryEngine should find B.java as a candidate for references to A"
    );
}

/// Capability 5: Method Call Tracking
/// The graph uses reference_index to discover method call references
#[test]
fn cap_method_call_tracking() {
    let files = vec![
        ("A.java", "public class A { void target() {} }"),
        ("B.java", "public class B { void m(A a) { a.target(); } }"),
    ];
    let (index, _) = setup_java_test_graph(files);

    let a_target_idx = index.find_node("A#target").unwrap();

    // Check DiscoveryEngine "Scouting" (uses Reference Index)
    let discovery = DiscoveryEngine::new(&index, std::collections::HashMap::new());
    let candidate_files = discovery.scout_references(&[a_target_idx]);
    assert!(
        candidate_files.contains(&std::path::PathBuf::from("B.java")),
        "DiscoveryEngine should find B.java as a candidate for calls to A.target"
    );
}

/// Capability 6: Interface Extension (InheritsFrom)
/// Interfaces extending other interfaces should use InheritsFrom edge
#[test]
fn cap_interface_extension() {
    let files = vec![
        ("Super.java", "public interface Super {}"),
        ("Sub.java", "public interface Sub extends Super {}"),
    ];
    let (index, _) = setup_java_test_graph(files);

    let super_idx = index.find_node("Super").unwrap();
    let sub_idx = index.find_node("Sub").unwrap();

    let has_inherits = index
        .topology()
        .edges_connecting(sub_idx, super_idx)
        .any(|e| e.weight().edge_type == EdgeType::InheritsFrom);

    assert!(
        has_inherits,
        "Interface 'Sub' should have InheritsFrom edge to 'Super'"
    );
}

/// Capability 7: Annotation Tracking (DecoratedBy)
/// Annotations should be linked to their targets
#[test]
fn cap_annotation_usage() {
    let files = vec![
        ("MyAnno.java", "public @interface MyAnno {}"),
        ("App.java", "@MyAnno public class App {}"),
    ];
    let (index, _) = setup_java_test_graph(files);

    let app_idx = index.find_node("App").unwrap();
    let anno_idx = index.find_node("MyAnno").unwrap();

    let has_decorated = index
        .topology()
        .edges_connecting(app_idx, anno_idx)
        .any(|e| e.weight().edge_type == EdgeType::DecoratedBy);

    assert!(
        has_decorated,
        "Class 'App' should have DecoratedBy edge to '@MyAnno'"
    );
}

/// Capability 8: Static Field Access
/// Tracking access to static members (e.g., Constants)
#[test]
fn cap_static_field_access() {
    let files = vec![
        (
            "Config.java",
            "public class Config { public static String KEY = \"v\"; }",
        ),
        ("Main.java", "public class Main { String s = Config.KEY; }"),
    ];
    let (index, _) = setup_java_test_graph(files);

    let config_key_idx = index.find_node("Config#KEY").unwrap();

    // Checking if Main.java is discovered as a candidate for Config.KEY
    let discovery = DiscoveryEngine::new(&index, std::collections::HashMap::new());
    let candidate_files = discovery.scout_references(&[config_key_idx]);
    assert!(
        candidate_files.contains(&std::path::PathBuf::from("Main.java")),
        "Main.java should be discovered as a candidate for Config.KEY"
    );
}

/// Capability 9: Generic Type Resolution (TypedAs)
/// Does the graph handle List<TypeA> by linking to TypeA?
#[test]
fn cap_generic_type_link() {
    let files = vec![
        ("TypeA.java", "public class TypeA {}"),
        (
            "Main.java",
            "import java.util.List; public class Main { java.util.List<TypeA> list; }",
        ),
    ];
    let (index, _) = setup_java_test_graph(files);

    let list_idx = index.find_node("Main#list").unwrap();
    let type_a_idx = index.find_node("TypeA").unwrap();

    let has_link = index
        .topology()
        .edges_connecting(list_idx, type_a_idx)
        .any(|e| e.weight().edge_type == EdgeType::TypedAs);

    assert!(
        has_link,
        "Generic argument 'TypeA' should be linked via TypedAs"
    );
}
