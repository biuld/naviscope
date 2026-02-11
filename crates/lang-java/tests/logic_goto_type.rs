mod common;

use common::{offset_to_point, setup_java_test_graph};
use naviscope_core::features::CodeGraphLike;
use naviscope_plugin::{SymbolQueryService, SymbolResolveService};
use naviscope_java::resolver::JavaResolver;

#[test]
fn test_goto_type_definition_variable() {
    let files = vec![
        ("Model.java", "public class Model {}"),
        (
            "Client.java",
            "public class Client { void m() { Model m = null; } }",
        ),
    ];
    let (index, trees) = setup_java_test_graph(files);
    let resolver = JavaResolver::new();

    let client_content = &trees[1].1;
    let client_tree = &trees[1].2;

    // Resolve type of 'm' in 'Model m'
    let usage_pos = client_content.find("m = null").unwrap();
    let (line, col) = offset_to_point(client_content, usage_pos);

    let res = resolver
        .resolve_at(client_tree, client_content, line, col, &index)
        .expect("Should resolve m");
    let type_res = resolver.resolve_type_of(&index, &res);

    assert!(!type_res.is_empty());
    let matches = resolver.find_matches(&index, &type_res[0]);
    assert!(!matches.is_empty());
    let idx = *index.fqn_map().get(&matches[0]).expect("Node not found");
    assert_eq!(
        index.render_fqn(
            &index.topology()[idx],
            Some(&naviscope_java::naming::JavaNamingConvention)
        ),
        "Model"
    );
}

#[test]
fn test_goto_type_definition_method_return() {
    let files = vec![
        ("Model.java", "public class Model {}"),
        (
            "Service.java",
            "public class Service { Model get() { return null; } }",
        ),
    ];
    let (index, trees) = setup_java_test_graph(files);
    let resolver = JavaResolver::new();

    let service_content = &trees[1].1;
    let service_tree = &trees[1].2;

    // Resolve type of method 'get'
    let usage_pos = service_content.find("get()").unwrap();
    let (line, col) = offset_to_point(service_content, usage_pos);

    let res = resolver
        .resolve_at(service_tree, service_content, line, col, &index)
        .expect("Should resolve get");
    let type_res = resolver.resolve_type_of(&index, &res);

    assert!(!type_res.is_empty());
    let matches = resolver.find_matches(&index, &type_res[0]);
    assert!(!matches.is_empty());
    let idx = *index.fqn_map().get(&matches[0]).expect("Node not found");
    assert_eq!(
        index.render_fqn(
            &index.topology()[idx],
            Some(&naviscope_java::naming::JavaNamingConvention)
        ),
        "Model"
    );
}
