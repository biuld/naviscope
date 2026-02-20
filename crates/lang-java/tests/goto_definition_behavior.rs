mod common;

use common::{offset_to_point, setup_java_test_graph};
use naviscope_api::models::SymbolResolution;
use naviscope_core::features::CodeGraphLike;
use naviscope_java::JavaPlugin;
use naviscope_plugin::{SymbolQueryService, SymbolResolveService};

#[test]
fn given_local_usage_when_goto_definition_then_returns_local_binding() {
    let files = vec![(
        "Test.java",
        "public class Test { void main() { int x = 1; int y = x + 1; } }",
    )];

    let (index, trees) = setup_java_test_graph(files);
    let resolver = JavaPlugin::new().expect("Failed to create JavaPlugin");

    let content = &trees[0].1;
    let tree = &trees[0].2;
    let usage_pos = content.rfind("x + 1").expect("find usage");
    let (line, col) = offset_to_point(content, usage_pos);

    let resolution = resolver
        .resolve_at(tree, content, line, col, &index)
        .expect("resolve local symbol");

    if let SymbolResolution::Local(range, _) = resolution {
        let def_pos = content.find("int x").expect("find declaration") + 4;
        assert_eq!(range.start_col, def_pos);
    } else {
        panic!("expected local resolution");
    }
}

#[test]
fn given_cross_file_call_when_goto_definition_then_resolves_precise_method_owner() {
    let files = vec![
        (
            "A.java",
            "package com; public class A { public void hello() {} }",
        ),
        (
            "B.java",
            "package com; public class B { void run(A a) { a.hello(); } }",
        ),
    ];

    let (index, trees) = setup_java_test_graph(files);
    let resolver = JavaPlugin::new().expect("Failed to create JavaPlugin");

    let content = &trees[1].1;
    let tree = &trees[1].2;
    let pos = content.find("hello()").expect("find method call");
    let (line, col) = offset_to_point(content, pos);

    let resolution = resolver
        .resolve_at(tree, content, line, col, &index)
        .expect("resolve hello call");
    let matches = resolver.find_matches(&index, &resolution);

    assert_eq!(matches.len(), 1, "goto definition should be unique");
    let idx = *index.fqn_map().get(&matches[0]).expect("node exists");
    assert_eq!(
        index.render_fqn(
            &index.topology()[idx],
            Some(&naviscope_java::naming::JavaNamingConvention::default())
        ),
        "com.A#hello()"
    );
}

#[test]
fn given_shadowed_local_when_goto_definition_then_binds_to_local_declaration() {
    let files = vec![(
        "Test.java",
        "public class Test { int x = 0; void run() { int x = 1; x = 2; } }",
    )];

    let (index, trees) = setup_java_test_graph(files);
    let resolver = JavaPlugin::new().expect("Failed to create JavaPlugin");

    let content = &trees[0].1;
    let tree = &trees[0].2;
    let usage_pos = content.find("x = 2").expect("find local assignment");
    let (line, col) = offset_to_point(content, usage_pos);

    let resolution = resolver
        .resolve_at(tree, content, line, col, &index)
        .expect("resolve local usage");

    if let SymbolResolution::Local(range, _) = resolution {
        let def_pos = content.find("int x = 1").expect("find local declaration") + 4;
        assert_eq!(range.start_col, def_pos);
    } else {
        panic!("expected local resolution for shadowed variable");
    }
}

#[test]
fn given_constructor_call_when_goto_definition_then_resolves_class_or_ctor_symbol() {
    let files = vec![
        ("A.java", "public class A { public A() {} }"),
        ("B.java", "public class B { A a = new A(); }"),
    ];

    let (index, trees) = setup_java_test_graph(files);
    let resolver = JavaPlugin::new().expect("Failed to create JavaPlugin");

    let b_content = &trees[1].1;
    let b_tree = &trees[1].2;
    let usage_pos = b_content.find("new A()").expect("find constructor call") + 4;
    let (line, col) = offset_to_point(b_content, usage_pos);

    let resolution = resolver
        .resolve_at(b_tree, b_content, line, col, &index)
        .expect("resolve constructor symbol");
    let matches = resolver.find_matches(&index, &resolution);

    assert!(!matches.is_empty());
    let idx = *index.fqn_map().get(&matches[0]).expect("node exists");
    assert!(
        index
            .render_fqn(
                &index.topology()[idx],
                Some(&naviscope_java::naming::JavaNamingConvention::default())
            )
            .contains('A')
    );
}

#[test]
fn given_static_field_access_when_goto_definition_then_resolves_declaring_field() {
    let files = vec![
        ("A.java", "public class A { public static int VAL = 1; }"),
        ("B.java", "public class B { int x = A.VAL; }"),
    ];

    let (index, trees) = setup_java_test_graph(files);
    let resolver = JavaPlugin::new().expect("Failed to create JavaPlugin");

    let b_content = &trees[1].1;
    let b_tree = &trees[1].2;
    let usage_pos = b_content.find("VAL").expect("find static field usage");
    let (line, col) = offset_to_point(b_content, usage_pos);

    let resolution = resolver
        .resolve_at(b_tree, b_content, line, col, &index)
        .expect("resolve static field");
    let matches = resolver.find_matches(&index, &resolution);

    assert!(!matches.is_empty());
    let idx = *index.fqn_map().get(&matches[0]).expect("node exists");
    assert_eq!(
        index.render_fqn(
            &index.topology()[idx],
            Some(&naviscope_java::naming::JavaNamingConvention::default())
        ),
        "A#VAL"
    );
}

#[test]
fn given_overloaded_method_call_chain_when_goto_definition_then_uses_most_specific_overload() {
    let files = vec![
        (
            "BaseResult.java",
            "public class BaseResult { void base() {} }",
        ),
        (
            "SpecialResult.java",
            "public class SpecialResult extends BaseResult { void special() {} }",
        ),
        ("SpecialArg.java", "public class SpecialArg {}"),
        (
            "A.java",
            "public class A { BaseResult pick(Object o) { return new BaseResult(); } SpecialResult pick(SpecialArg a) { return new SpecialResult(); } }",
        ),
        (
            "Use.java",
            "public class Use { void run(A a) { a.pick(new SpecialArg()).special(); } }",
        ),
    ];

    let (index, trees) = setup_java_test_graph(files);
    let resolver = JavaPlugin::new().expect("Failed to create JavaPlugin");

    let content = &trees[4].1;
    let tree = &trees[4].2;
    let pos = content
        .find("special()")
        .expect("find chained method invocation");
    let (line, col) = offset_to_point(content, pos);

    let resolution = resolver
        .resolve_at(tree, content, line, col, &index)
        .expect("resolve chained method symbol");
    let matches = resolver.find_matches(&index, &resolution);

    assert_eq!(matches.len(), 1);
    let idx = *index.fqn_map().get(&matches[0]).expect("node exists");
    assert_eq!(
        index.render_fqn(
            &index.topology()[idx],
            Some(&naviscope_java::naming::JavaNamingConvention::default())
        ),
        "SpecialResult#special()"
    );
}

#[test]
fn given_overloaded_constructor_call_when_goto_definition_then_resolves_constructor_type_symbol() {
    let files = vec![
        ("SpecialArg.java", "public class SpecialArg {}"),
        ("A.java", "public class A { A() {} A(SpecialArg arg) {} }"),
        (
            "Use.java",
            "public class Use { A build() { return new A(new SpecialArg()); } }",
        ),
    ];

    let (index, trees) = setup_java_test_graph(files);
    let resolver = JavaPlugin::new().expect("Failed to create JavaPlugin");

    let content = &trees[2].1;
    let tree = &trees[2].2;
    let pos = content
        .find("new A(new SpecialArg())")
        .expect("find overloaded constructor call")
        + 4;
    let (line, col) = offset_to_point(content, pos);

    let resolution = resolver
        .resolve_at(tree, content, line, col, &index)
        .expect("resolve constructor symbol");
    let matches = resolver.find_matches(&index, &resolution);

    assert!(!matches.is_empty());
    let idx = *index.fqn_map().get(&matches[0]).expect("node exists");
    assert_eq!(
        index.render_fqn(
            &index.topology()[idx],
            Some(&naviscope_java::naming::JavaNamingConvention::default())
        ),
        "A"
    );
}

#[test]
fn given_same_class_different_arity_overloads_when_goto_definition_then_resolves_member_symbol() {
    let files = vec![(
        "A.java",
        "public class A { void target() { target(1); target(1, 2); } void target(int a) {} void target(int a, int b) {} }",
    )];

    let (index, trees) = setup_java_test_graph(files);
    let resolver = JavaPlugin::new().expect("Failed to create JavaPlugin");

    let content = &trees[0].1;
    let tree = &trees[0].2;
    let pos = content
        .find("target(1, 2)")
        .expect("find same-class overload call");
    let (line, col) = offset_to_point(content, pos);

    let resolution = resolver
        .resolve_at(tree, content, line, col, &index)
        .expect("resolve overloaded call");
    let matches = resolver.find_matches(&index, &resolution);

    assert!(!matches.is_empty());
    let idx = *index.fqn_map().get(&matches[0]).expect("node exists");
    assert_eq!(
        index.render_fqn(
            &index.topology()[idx],
            Some(&naviscope_java::naming::JavaNamingConvention::default())
        ),
        "A#target(int,int)"
    );
}
