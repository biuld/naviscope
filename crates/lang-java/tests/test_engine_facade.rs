mod common;

use common::{offset_to_point, setup_java_engine};
use naviscope_api::models::{PositionContext, ReferenceQuery, SymbolQuery, SymbolResolution};
use naviscope_api::semantic::{
    CallHierarchyAnalyzer, ReferenceAnalyzer, SymbolInfoProvider, SymbolNavigator,
};
use std::collections::BTreeSet;

#[tokio::test]
async fn test_full_engine_java_facade() {
    let temp_dir = std::env::temp_dir().join("naviscope_java_facade_test");
    if temp_dir.exists() {
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
    std::fs::create_dir_all(&temp_dir).unwrap();

    let files = vec![
        (
            "com/example/Base.java",
            "package com.example; public interface Base { void run(); }",
        ),
        (
            "com/example/Impl.java",
            "package com.example; public class Impl implements Base { public void run() {} }",
        ),
        (
            "com/example/App.java",
            r#"
package com.example;
public class App {
    void start() {
        Base b = new Impl();
        b.run();
    }
}
"#,
        ),
    ];

    let handle = setup_java_engine(&temp_dir, files).await;
    let graph = handle.graph().await;
    graph.register_naming_convention(Box::new(
        naviscope_plugin::StandardNamingConvention::default(),
    ));
    graph.topology();

    let app_path = temp_dir.join("com/example/App.java");
    let app_content = std::fs::read_to_string(&app_path).unwrap();
    let run_pos = app_content.find("b.run()").unwrap() + 2;
    let (line, col) = offset_to_point(&app_content, run_pos);

    let ctx = PositionContext {
        uri: format!("file://{}", app_path.display()),
        line: line as u32,
        char: col as u32,
        content: Some(app_content.clone()),
    };

    let resolution = handle
        .resolve_symbol_at(&ctx)
        .await
        .unwrap()
        .expect("Should resolve b.run()");

    match &resolution {
        SymbolResolution::Precise(fqn, _) => assert_eq!(fqn, "com.example.Base#run()"),
        SymbolResolution::Global(fqn) => assert_eq!(fqn, "com.example.Base#run()"),
        _ => panic!(
            "Expected precise or global resolution, got {:?}",
            resolution
        ),
    }

    let query = SymbolQuery {
        language: naviscope_api::models::Language::JAVA,
        resolution: resolution.clone(),
    };
    let impls = handle.find_implementations(&query).await.unwrap();
    assert_eq!(impls.len(), 1);
    assert!(impls[0].path.to_string_lossy().contains("Impl.java"));

    let calls = handle
        .find_incoming_calls("com.example.Impl#run()")
        .await
        .unwrap();
    assert_eq!(
        calls.len(),
        1,
        "Lookup of Impl#run should find the call via Base type"
    );

    let incoming_base = handle
        .find_incoming_calls("com.example.Base#run()")
        .await
        .unwrap();

    let query_refs = ReferenceQuery {
        language: naviscope_api::models::Language::JAVA,
        resolution: resolution.clone(),
        include_declaration: false,
    };
    let refs = handle.find_references(&query_refs).await.unwrap();
    assert_eq!(
        refs.len(),
        1,
        "find_references should have found 1 reference in App.java"
    );

    assert_eq!(
        incoming_base.len(),
        1,
        "find_incoming_calls should have found 1 caller in App.java"
    );
    assert_eq!(incoming_base[0].from.id, "com.example.App#start()");
}

#[tokio::test]
async fn test_find_references_filters_same_name_across_types() {
    let temp_dir = std::env::temp_dir().join("naviscope_java_ref_same_name_test");
    if temp_dir.exists() {
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
    std::fs::create_dir_all(&temp_dir).unwrap();

    let files = vec![
        (
            "com/example/A.java",
            "package com.example; public class A { public void target() {} }",
        ),
        (
            "com/example/X.java",
            "package com.example; public class X { public void target() {} }",
        ),
        (
            "com/example/Use.java",
            r#"
package com.example;
public class Use {
    void useA(A a) { a.target(); }
    void useX(X x) { x.target(); }
}
"#,
        ),
    ];

    let handle = setup_java_engine(&temp_dir, files).await;

    let a_path = temp_dir.join("com/example/A.java");
    let a_content = std::fs::read_to_string(&a_path).unwrap();
    let pos = a_content.find("target()").unwrap();
    let (line, col) = offset_to_point(&a_content, pos);

    let ctx = PositionContext {
        uri: format!("file://{}", a_path.display()),
        line: line as u32,
        char: col as u32,
        content: Some(a_content.clone()),
    };

    let resolution = handle
        .resolve_symbol_at(&ctx)
        .await
        .unwrap()
        .expect("Should resolve A#target");

    let refs = handle
        .find_references(&ReferenceQuery {
            language: naviscope_api::models::Language::JAVA,
            resolution,
            include_declaration: false,
        })
        .await
        .unwrap();

    assert_eq!(refs.len(), 1, "A#target should only match a.target() usage");
    assert!(refs[0].path.to_string_lossy().contains("Use.java"));

    let use_content = std::fs::read_to_string(temp_dir.join("com/example/Use.java")).unwrap();
    let prefix = use_content
        .lines()
        .take(refs[0].range.start_line + 1)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        prefix.contains("useA"),
        "Reference should belong to useA(A), not useX(X)"
    );
}

#[tokio::test]
async fn test_find_references_include_declaration_switch() {
    let temp_dir = std::env::temp_dir().join("naviscope_java_ref_include_decl_test");
    if temp_dir.exists() {
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
    std::fs::create_dir_all(&temp_dir).unwrap();

    let files = vec![
        (
            "com/example/A.java",
            "package com.example; public class A { public void target() {} }",
        ),
        (
            "com/example/Use.java",
            r#"
package com.example;
public class Use {
    void useA(A a) { a.target(); }
}
"#,
        ),
    ];

    let handle = setup_java_engine(&temp_dir, files).await;

    let a_path = temp_dir.join("com/example/A.java");
    let a_content = std::fs::read_to_string(&a_path).unwrap();
    let pos = a_content.find("target()").unwrap();
    let (line, col) = offset_to_point(&a_content, pos);

    let ctx = PositionContext {
        uri: format!("file://{}", a_path.display()),
        line: line as u32,
        char: col as u32,
        content: Some(a_content.clone()),
    };

    let resolution = handle
        .resolve_symbol_at(&ctx)
        .await
        .unwrap()
        .expect("Should resolve A#target");

    let refs_without_decl = handle
        .find_references(&ReferenceQuery {
            language: naviscope_api::models::Language::JAVA,
            resolution: resolution.clone(),
            include_declaration: false,
        })
        .await
        .unwrap();
    assert_eq!(refs_without_decl.len(), 1);

    let refs_with_decl = handle
        .find_references(&ReferenceQuery {
            language: naviscope_api::models::Language::JAVA,
            resolution,
            include_declaration: true,
        })
        .await
        .unwrap();
    assert_eq!(refs_with_decl.len(), 2);
}

#[tokio::test]
async fn test_find_references_static_member_hiding() {
    let temp_dir = std::env::temp_dir().join("naviscope_java_ref_static_hiding_test");
    if temp_dir.exists() {
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
    std::fs::create_dir_all(&temp_dir).unwrap();

    let files = vec![
        (
            "com/example/Base.java",
            "package com.example; public class Base { public static void ping() {} }",
        ),
        (
            "com/example/Child.java",
            "package com.example; public class Child extends Base { public static void ping() {} }",
        ),
        (
            "com/example/Use.java",
            r#"
package com.example;
public class Use {
    void use() {
        Base.ping();
        Child.ping();
    }
}
"#,
        ),
    ];

    let handle = setup_java_engine(&temp_dir, files).await;

    let base_path = temp_dir.join("com/example/Base.java");
    let base_content = std::fs::read_to_string(&base_path).unwrap();
    let pos = base_content.find("ping()").unwrap();
    let (line, col) = offset_to_point(&base_content, pos);

    let ctx = PositionContext {
        uri: format!("file://{}", base_path.display()),
        line: line as u32,
        char: col as u32,
        content: Some(base_content.clone()),
    };

    let resolution = handle
        .resolve_symbol_at(&ctx)
        .await
        .unwrap()
        .expect("Should resolve Base#ping");

    let refs = handle
        .find_references(&ReferenceQuery {
            language: naviscope_api::models::Language::JAVA,
            resolution,
            include_declaration: false,
        })
        .await
        .unwrap();

    assert_eq!(
        refs.len(),
        1,
        "Base#ping should only match Base.ping() usage"
    );
    assert!(refs[0].path.to_string_lossy().contains("Use.java"));

    let use_content = std::fs::read_to_string(temp_dir.join("com/example/Use.java")).unwrap();
    let prefix = use_content
        .lines()
        .take(refs[0].range.start_line + 1)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        prefix.contains("Base.ping"),
        "Reference should belong to Base.ping(), not Child.ping()"
    );
}

#[tokio::test]
async fn test_find_references_same_class_overloads_different_arity_via_engine_facade() {
    let temp_dir = std::env::temp_dir().join("naviscope_java_ref_overload_arity_test");
    if temp_dir.exists() {
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
    std::fs::create_dir_all(&temp_dir).unwrap();

    let files = vec![(
        "com/example/A.java",
        "package com.example; public class A { void target() { target(1); target(1, 2); } void target(int a) {} void target(int a, int b) {} }",
    )];

    let handle = setup_java_engine(&temp_dir, files).await;

    let a_path = temp_dir.join("com/example/A.java");
    let content = std::fs::read_to_string(&a_path).unwrap();
    let pos = content.find("target() {").unwrap();
    let (line, col) = offset_to_point(&content, pos);

    let ctx = PositionContext {
        uri: format!("file://{}", a_path.display()),
        line: line as u32,
        char: col as u32,
        content: Some(content.clone()),
    };

    let resolution = handle
        .resolve_symbol_at(&ctx)
        .await
        .unwrap()
        .expect("Should resolve A#target");

    let refs = handle
        .find_references(&ReferenceQuery {
            language: naviscope_api::models::Language::JAVA,
            resolution,
            include_declaration: true,
        })
        .await
        .unwrap();

    assert_eq!(
        refs.len(),
        1,
        "expected strict resolution to match ONLY exact overload decl (target() is never called)"
    );
    assert!(
        refs.iter()
            .all(|r| r.path.to_string_lossy().contains("com/example/A.java")),
        "all references should stay in A.java for this scenario"
    );

    let starts: BTreeSet<(usize, usize)> = refs
        .iter()
        .map(|r| (r.range.start_line, r.range.start_col))
        .collect();
    let expected: BTreeSet<(usize, usize)> = [content.find("target() {").unwrap()]
        .into_iter()
        .map(|offset| offset_to_point(&content, offset))
        .collect();
    assert_eq!(starts, expected);
}

#[tokio::test]
async fn test_resolve_method_declaration_name_and_symbol_info_for_overload_hover_input() {
    let temp_dir = std::env::temp_dir().join("naviscope_java_hover_decl_overload_test");
    if temp_dir.exists() {
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
    std::fs::create_dir_all(&temp_dir).unwrap();

    let files = vec![(
        "com/example/A.java",
        "package com.example; public class A { void target(int a) {} void target(int a, int b) {} }",
    )];

    let handle = setup_java_engine(&temp_dir, files).await;

    let a_path = temp_dir.join("com/example/A.java");
    let content = std::fs::read_to_string(&a_path).unwrap();
    let pos = content.find("target(int a)").unwrap();
    let (line, col) = offset_to_point(&content, pos);

    let ctx = PositionContext {
        uri: format!("file://{}", a_path.display()),
        line: line as u32,
        char: col as u32,
        content: Some(content.clone()),
    };

    let resolution = handle
        .resolve_symbol_at(&ctx)
        .await
        .unwrap()
        .expect("Should resolve method declaration name");

    let fqn = match resolution {
        SymbolResolution::Precise(fqn, _) | SymbolResolution::Global(fqn) => fqn,
        SymbolResolution::Local(_, _) => panic!("method declaration should not resolve as local"),
    };
    assert_eq!(fqn, "com.example.A#target(int)");

    let info = handle
        .get_symbol_info(&fqn)
        .await
        .unwrap()
        .expect("symbol info should exist for hover input");

    let sig = info.signature.unwrap_or_default();
    assert!(
        sig.contains("target("),
        "signature should be suitable for hover at declaration name"
    );
}
