mod common;

use common::{offset_to_point, setup_java_test_graph};
use naviscope_java::JavaPlugin;
use naviscope_plugin::{SymbolQueryService, SymbolResolveService};

#[test]
fn test_cross_file_resolution() {
    let files = vec![
        (
            "src/main/java/com/example/A.java",
            "package com.example; public class A { public void hello() {} }",
        ),
        (
            "src/main/java/com/example/B.java",
            "package com.example; public class B { void test() { A a = new A(); a.hello(); } }",
        ),
    ];

    let (index, trees) = setup_java_test_graph(files);
    let resolver = JavaPlugin::new().expect("Failed to create JavaPlugin");

    // Test resolving 'A' in 'A a = new A();'
    let b_content = &trees[1].1;
    let b_tree = &trees[1].2;

    // Find 'A' in 'A a'
    let a_pos = b_content
        .find("A a")
        .expect("Could not find 'A a' in B.java");
    println!("Found 'A a' at byte offset {}", a_pos);

    let res = resolver.resolve_at(b_tree, b_content, 0, a_pos, &index);
    assert!(res.is_some(), "Failed to resolve 'A' at {}", a_pos);
    if let Some(naviscope_core::ingest::parser::SymbolResolution::Precise(fqn, _)) = res {
        assert_eq!(fqn, "com.example.A");
    } else {
        panic!(
            "Expected precise resolution to com.example.A, got {:?}",
            res
        );
    }

    // Test resolving 'hello' in 'a.hello();'
    let hello_pos = b_content
        .find("hello();")
        .expect("Could not find 'hello();' in B.java");
    println!("Found 'hello();' at byte offset {}", hello_pos);

    let res = resolver.resolve_at(b_tree, b_content, 0, hello_pos, &index);
    assert!(res.is_some(), "Failed to resolve 'hello' at {}", hello_pos);
    if let Some(naviscope_core::ingest::parser::SymbolResolution::Precise(fqn, _)) = res {
        assert_eq!(fqn, "com.example.A#hello");
    } else {
        panic!(
            "Expected precise resolution to com.example.A.hello, got {:?}",
            res
        );
    }
}

#[test]
fn test_inheritance_and_implementations() {
    let files = vec![
        ("I.java", "public interface I { void run(); }"),
        (
            "C.java",
            "public class C implements I { public void run() {} }",
        ),
    ];

    let (index, trees) = setup_java_test_graph(files);
    let resolver = JavaPlugin::new().expect("Failed to create JavaPlugin");

    let i_content = &trees[0].1;
    let i_tree = &trees[0].2;

    // Resolve 'I' in its definition
    let i_pos = i_content
        .find("interface I")
        .expect("Could not find 'interface I'")
        + "interface ".len();
    let res = resolver.resolve_at(i_tree, i_content, 0, i_pos, &index);
    assert!(res.is_some(), "Failed to resolve 'I' at {}", i_pos);
    let res = res.unwrap();

    let impls = resolver.find_implementations(&index, &res);
    assert_eq!(impls.len(), 1);

    let node_idx = *index.fqn_map().get(&impls[0]).expect("Node not found");
    let node = &index.topology()[node_idx];
    assert_eq!(
        {
            use naviscope_plugin::NamingConvention;
            naviscope_plugin::StandardNamingConvention.render_fqn(node.id, index.fqns())
        },
        "C"
    );
}

#[test]
fn test_inner_class_resolution() {
    let files = vec![
        (
            "src/example/Outer.java",
            "package com.example; public class Outer { public class Inner { public void innerMethod() {} } }",
        ),
        (
            "src/example/Client.java",
            "package com.example; public class Client { void test() { Outer.Inner inner; } }",
        ),
    ];

    let (index, trees) = setup_java_test_graph(files);
    let resolver = JavaPlugin::new().expect("Failed to create JavaPlugin");

    let client_content = &trees[1].1;
    let client_tree = &trees[1].2;

    // Resolve 'Inner' in 'Outer.Inner'
    let inner_pos = client_content
        .find("Inner inner")
        .expect("Could not find 'Inner inner'");
    let res = resolver.resolve_at(client_tree, client_content, 0, inner_pos, &index);

    assert!(res.is_some(), "Failed to resolve 'Inner' at {}", inner_pos);
    if let Some(naviscope_core::ingest::parser::SymbolResolution::Precise(fqn, _)) = res {
        assert_eq!(fqn, "com.example.Outer.Inner");
    } else {
        panic!(
            "Expected precise resolution to com.example.Outer.Inner, got {:?}",
            res
        );
    }
}

#[test]
fn test_chained_calls_resolution() {
    let files = vec![
        (
            "src/chain/A.java",
            "package com.chain; public class A { public B getB() { return new B(); } }",
        ),
        (
            "src/chain/B.java",
            "package com.chain; public class B { public C getC() { return new C(); } }",
        ),
        (
            "src/chain/C.java",
            "package com.chain; public class C { public void execute() {} }",
        ),
        (
            "src/chain/Main.java",
            "package com.chain; public class Main { void run() { A a = new A(); a.getB().getC().execute(); } }",
        ),
    ];

    let (index, trees) = setup_java_test_graph(files);
    let resolver = JavaPlugin::new().expect("Failed to create JavaPlugin");

    let main_content = &trees[3].1;
    let main_tree = &trees[3].2;

    // Position of 'getC' in 'a.getB().getC().execute()'
    let get_c_pos = main_content
        .find("getC()")
        .expect("Could not find 'getC()'");
    let res = resolver.resolve_at(main_tree, main_content, 0, get_c_pos, &index);
    assert!(res.is_some(), "Failed to resolve 'getC' at {}", get_c_pos);
    if let Some(naviscope_core::ingest::parser::SymbolResolution::Precise(fqn, _)) = res {
        assert_eq!(fqn, "com.chain.B#getC");
    } else {
        panic!(
            "Expected precise resolution to com.chain.B.getC, got {:?}",
            res
        );
    }

    // Position of 'execute' in 'a.getB().getC().execute()'
    let execute_pos = main_content
        .find("execute()")
        .expect("Could not find 'execute()'");
    let res = resolver.resolve_at(main_tree, main_content, 0, execute_pos, &index);
    assert!(
        res.is_some(),
        "Failed to resolve 'execute' at {}",
        execute_pos
    );
    if let Some(naviscope_core::ingest::parser::SymbolResolution::Precise(fqn, _)) = res {
        assert_eq!(fqn, "com.chain.C#execute");
    } else {
        panic!(
            "Expected precise resolution to com.chain.C.execute, got {:?}",
            res
        );
    }
}

#[test]
fn test_hover_chain_middle_node_resolution() {
    let files = vec![
        (
            "src/web/HttpResponse.java",
            "package com.web; public class HttpResponse { public SessionContext getContext() { return new SessionContext(); } }",
        ),
        (
            "src/web/SessionContext.java",
            "package com.web; public class SessionContext { public Object get(String key) { return null; } }",
        ),
        (
            "src/web/Main.java",
            r#"package com.web;
public class Main {
    void run() {
        HttpResponse response = new HttpResponse();
        response.getContext().get("key");
    }
}"#,
        ),
    ];

    let (index, trees) = setup_java_test_graph(files);
    let resolver = JavaPlugin::new().expect("Failed to create JavaPlugin");

    let main_content = &trees[2].1;
    let main_tree = &trees[2].2;

    let get_context_pos = main_content
        .find("getContext()")
        .expect("Could not find 'getContext()'");
    let (line, col) = offset_to_point(main_content, get_context_pos);

    let res = resolver.resolve_at(main_tree, main_content, line, col, &index);
    assert!(
        res.is_some(),
        "Failed to resolve chain middle node 'getContext'"
    );

    if let Some(naviscope_core::ingest::parser::SymbolResolution::Precise(fqn, _)) = res {
        assert_eq!(fqn, "com.web.HttpResponse#getContext");
    } else {
        panic!(
            "Expected precise resolution to com.web.HttpResponse#getContext, got {:?}",
            res
        );
    }
}

#[test]
fn test_lambda_parameter_resolution() {
    let files = vec![(
        "src/LambdaTest.java",
        "public class LambdaTest { void test() { java.util.List<String> list; list.forEach(it -> { String s = it; }); } }",
    )];

    let (index, trees) = setup_java_test_graph(files);
    let resolver = JavaPlugin::new().expect("Failed to create JavaPlugin");

    let content = &trees[0].1;
    let tree = &trees[0].2;

    // Resolve 'it' in 'String s = it;'
    let it_usage_pos = content.find("s = it").expect("Could not find 's = it'") + "s = ".len();
    let res = resolver.resolve_at(tree, content, 0, it_usage_pos, &index);

    assert!(
        res.is_some(),
        "Failed to resolve lambda parameter 'it' at {}",
        it_usage_pos
    );
    if let Some(naviscope_core::ingest::parser::SymbolResolution::Local(range, _)) = res {
        // The definition of 'it' should be at 'it ->'
        let it_def_pos = content.find("it ->").expect("Could not find 'it ->'");
        assert_eq!(range.start_col, it_def_pos);
    } else {
        panic!(
            "Expected local resolution for lambda parameter, got {:?}",
            res
        );
    }
}

#[test]
fn test_lambda_explicit_type_resolution() {
    let files = vec![
        (
            "src/A.java",
            "package com; public class A { public void hello() {} }",
        ),
        (
            "src/LambdaTypeTest.java",
            "package com; public class LambdaTypeTest { void test() { java.util.List<A> list; list.forEach((A it) -> { it.hello(); }); } }",
        ),
    ];

    let (index, trees) = setup_java_test_graph(files);
    let resolver = JavaPlugin::new().expect("Failed to create JavaPlugin");

    let content = &trees[1].1;
    let tree = &trees[1].2;

    // Resolve 'hello' in 'it.hello();'
    let hello_pos = content.find("hello()").expect("Could not find 'hello()'");
    let res = resolver.resolve_at(tree, content, 0, hello_pos, &index);

    assert!(
        res.is_some(),
        "Failed to resolve 'hello' on lambda parameter at {}",
        hello_pos
    );
    if let Some(naviscope_core::ingest::parser::SymbolResolution::Precise(fqn, _)) = res {
        assert_eq!(fqn, "com.A#hello");
    } else {
        panic!("Expected precise resolution for it.hello(), got {:?}", res);
    }
}

#[test]
fn test_lambda_heuristic_type_inference() {
    let files = vec![
        (
            "src/A.java",
            "package com; public class A { public void hello() {} }",
        ),
        (
            "src/LambdaHeuristicTest.java",
            "package com; public class LambdaHeuristicTest { void test() { java.util.List<com.A> list; list.forEach(it -> it.hello()); } }",
        ),
    ];

    let (index, trees) = setup_java_test_graph(files);
    let resolver = JavaPlugin::new().expect("Failed to create JavaPlugin");

    let content = &trees[1].1;
    let tree = &trees[1].2;

    // Resolve 'hello' in 'it.hello();'
    let hello_pos = content.find("hello()").expect("Could not find 'hello()'");
    let res = resolver.resolve_at(tree, content, 0, hello_pos, &index);

    assert!(
        res.is_some(),
        "Failed to resolve 'hello' on lambda parameter via heuristic at {}",
        hello_pos
    );
    if let Some(naviscope_core::ingest::parser::SymbolResolution::Precise(fqn, _)) = res {
        assert_eq!(fqn, "com.A#hello");
    } else {
        panic!(
            "Expected precise resolution for it.hello() via heuristic, got {:?}",
            res
        );
    }
}

#[test]
fn test_this_keyword_resolution() {
    let files = vec![(
        "src/DefaultApplicationArguments.java",
        r#"
public class DefaultApplicationArguments {
    private final Source source;
    
    public List<String> getNonOptionArgs() {
        return this.source.getNonOptionArgs();
    }
    
    private static class Source {
        public List<String> getNonOptionArgs() { return null; }
    }
}
"#,
    )];

    let (index, trees) = setup_java_test_graph(files);
    let resolver = JavaPlugin::new().expect("Failed to create JavaPlugin");

    let content = &trees[0].1;
    let tree = &trees[0].2;

    // Resolve 'this' in 'this.source.getNonOptionArgs()'
    let this_pos = content
        .find("this.source")
        .expect("Could not find 'this.source'");
    let (line, col) = offset_to_point(content, this_pos);

    let res = resolver.resolve_at(tree, content, line, col, &index);

    assert!(
        res.is_some(),
        "Failed to resolve 'this' at line {}, col {}",
        line,
        col
    );
    if let Some(naviscope_core::ingest::parser::SymbolResolution::Precise(fqn, _)) = res {
        assert_eq!(fqn, "DefaultApplicationArguments");
    } else {
        panic!("Expected precise resolution for 'this', got {:?}", res);
    }
}

#[test]
fn test_spring_boot_hover_scenario() {
    let content = r#"/*
 * Copyright 2012-present the original author or authors.
 */

package org.springframework.boot;

import java.util.List;
import org.springframework.core.env.SimpleCommandLinePropertySource;

public class DefaultApplicationArguments {

	private final Source source;

	public DefaultApplicationArguments(String... args) {
		this.source = new Source(args);
	}

	public List<String> getNonOptionArgs() {
		return this.source.getNonOptionArgs();
	}

	private static class Source extends SimpleCommandLinePropertySource {
		Source(String[] args) {
			super(args);
		}

		@Override
		public List<String> getNonOptionArgs() {
			return null;
		}
	}
}
"#;
    let files = vec![
        (
            "src/main/java/org/springframework/boot/DefaultApplicationArguments.java",
            content,
        ),
        (
            "src/main/java/org/springframework/core/env/SimpleCommandLinePropertySource.java",
            "package org.springframework.core.env; public class SimpleCommandLinePropertySource { public java.util.List<String> getNonOptionArgs() { return null; } }",
        ),
    ];

    let (index, trees) = setup_java_test_graph(files);
    let resolver = JavaPlugin::new().expect("Failed to create JavaPlugin");

    let tree = &trees[0].2;
    let source_content = &trees[0].1;

    // Line 18 (0-indexed) in our content is 'return this.source.getNonOptionArgs();'
    let method_call_pos = source_content
        .find("this.source.getNonOptionArgs()")
        .expect("Find expression")
        + "this.source.".len();

    let (line, col) = offset_to_point(source_content, method_call_pos);
    println!(
        "Testing Spring scenario at line {}, col {} (offset {})",
        line, col, method_call_pos
    );

    let res = resolver.resolve_at(tree, source_content, line, col, &index);

    if let Some(naviscope_core::ingest::parser::SymbolResolution::Precise(fqn, _)) = res {
        assert_eq!(
            fqn,
            "org.springframework.boot.DefaultApplicationArguments.Source#getNonOptionArgs"
        );
    } else {
        println!("Graph nodes:");
        for (fqn, idx) in index.fqn_map() {
            let node = &index.topology()[*idx];
            use naviscope_plugin::NamingConvention;
            println!(
                " - {} ({:?})",
                naviscope_plugin::StandardNamingConvention.render_fqn(*fqn, index.fqns()),
                node.kind()
            );
        }
        panic!("Failed to resolve Spring Boot scenario, got {:?}", res);
    }
}

#[test]
fn test_field_method_call_resolution() {
    let files = vec![
        (
            "src/A.java",
            "public class A { private B b; public void doA() { b.doB(); } }",
        ),
        ("src/B.java", "public class B { public void doB() {} }"),
    ];

    let (index, trees) = setup_java_test_graph(files);
    let resolver = JavaPlugin::new().expect("Failed to create JavaPlugin");

    let a_content = &trees[0].1;
    let a_tree = &trees[0].2;

    // Resolve 'doB' in 'b.doB()'
    let do_b_pos = a_content.find("doB()").expect("Could not find 'doB()'");

    let (line, col) = offset_to_point(a_content, do_b_pos);

    let res = resolver.resolve_at(a_tree, a_content, line, col, &index);

    assert!(
        res.is_some(),
        "Failed to resolve 'doB' at line {}, col {}",
        line,
        col
    );
    if let Some(naviscope_core::ingest::parser::SymbolResolution::Precise(fqn, _)) = res {
        assert_eq!(fqn, "B#doB");
    } else {
        panic!("Expected precise resolution to B.doB, got {:?}", res);
    }
}
