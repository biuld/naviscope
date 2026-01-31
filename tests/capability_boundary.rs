mod common;

use common::setup_java_test_graph;
use naviscope::analysis::discovery::DiscoveryEngine;
use naviscope::model::graph::EdgeType;

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
    // Note: JavaResolver prepends "module::root." to packages when no specific module is found
    assert!(index.fqn_map().contains_key("module::root.com.example"));
    assert!(index.fqn_map().contains_key("com.example.MyClass"));
    assert!(index.fqn_map().contains_key("com.example.MyClass.field"));
    assert!(index.fqn_map().contains_key("com.example.MyClass.method"));

    // Assert nesting via 'Contains' edges
    let class_idx = index.fqn_map()["com.example.MyClass"];
    let pkg_idx = index.fqn_map()["module::root.com.example"];

    assert!(index.topology().contains_edge(pkg_idx, class_idx));

    let field_idx = index.fqn_map()["com.example.MyClass.field"];
    let method_idx = index.fqn_map()["com.example.MyClass.method"];
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

    let base_idx = index.fqn_map()["Base"];
    let impl_idx = index.fqn_map()["Impl"];

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

    let field_idx = index.fqn_map()["com.app.Main.field"];
    let type_a_idx = index.fqn_map()["com.lib.TypeA"];

    let has_typed_as = index
        .topology()
        .edges_connecting(field_idx, type_a_idx)
        .any(|e| e.weight().edge_type == EdgeType::TypedAs);

    assert!(
        has_typed_as,
        "Field 'Main.field' should be linked to 'TypeA' via TypedAs edge"
    );
}

/// Capability 4: Direct Instantiation (Instantiates)
/// The graph MUST track where classes are instantiated
#[test]
fn cap_instantiation_tracking() {
    let files = vec![
        ("A.java", "public class A {}"),
        ("B.java", "public class B { void m() { A a = new A(); } }"),
    ];
    let (index, _) = setup_java_test_graph(files);

    let b_m_idx = index.fqn_map()["B.m"];
    let a_idx = index.fqn_map()["A"];

    // 1. Check Meso-graph (Structural only - should NOT have the edge now)
    let has_instantiates_edge = index
        .topology()
        .edges_connecting(b_m_idx, a_idx)
        .any(|e| e.weight().edge_type == EdgeType::Instantiates);
    assert!(
        !has_instantiates_edge,
        "Meso-graph should NOT have direct Instantiates edge after pruning"
    );

    // 2. Check DiscoveryEngine "Scouting" (uses Reference Index)
    let discovery = DiscoveryEngine::new(&index);
    let candidate_files = discovery.scout_references(&[a_idx]);
    assert!(
        candidate_files.contains(&std::path::PathBuf::from("B.java")),
        "DiscoveryEngine should find B.java as a candidate for references to A"
    );
}

/// Capability 5: Method Call Tracking (Calls)
/// The graph SHOULD track method calls (This is the most complex part of indexing)
#[test]
fn cap_method_call_tracking() {
    let files = vec![
        ("A.java", "public class A { void target() {} }"),
        ("B.java", "public class B { void m(A a) { a.target(); } }"),
    ];
    let (index, _) = setup_java_test_graph(files);

    let b_m_idx = index.fqn_map()["B.m"];
    let a_target_idx = index.fqn_map()["A.target"];

    // 1. Check Meso-graph (Structural only - should NOT have the edge now)
    let has_calls_edge = index
        .topology()
        .edges_connecting(b_m_idx, a_target_idx)
        .any(|e| e.weight().edge_type == EdgeType::Calls);
    assert!(
        !has_calls_edge,
        "Meso-graph should NOT have direct Calls edge after pruning"
    );

    // 2. Check DiscoveryEngine "Scouting" (uses Reference Index)
    let discovery = DiscoveryEngine::new(&index);
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

    let super_idx = index.fqn_map()["Super"];
    let sub_idx = index.fqn_map()["Sub"];

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

    let app_idx = index.fqn_map()["App"];
    let anno_idx = index.fqn_map()["MyAnno"];

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

    let main_s_idx = index.fqn_map()["Main.s"];
    let config_key_idx = index.fqn_map()["Config.KEY"];

    // Checking if Main.java is discovered as a candidate for Config.KEY
    let discovery = DiscoveryEngine::new(&index);
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

    let list_idx = index.fqn_map()["Main.list"];
    let type_a_idx = index.fqn_map()["TypeA"];

    let has_link = index
        .topology()
        .edges_connecting(list_idx, type_a_idx)
        .any(|e| e.weight().edge_type == EdgeType::TypedAs);

    assert!(
        has_link,
        "Generic argument 'TypeA' should be linked via TypedAs"
    );
}
