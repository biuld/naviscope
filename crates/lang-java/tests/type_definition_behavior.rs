mod common;

use common::{offset_to_point, setup_java_test_graph};
use naviscope_core::features::CodeGraphLike;
use naviscope_java::JavaPlugin;
use naviscope_plugin::{SymbolQueryService, SymbolResolveService};

#[test]
fn given_variable_usage_when_goto_type_definition_then_returns_declared_type() {
    let files = vec![
        ("Model.java", "public class Model {}"),
        (
            "Client.java",
            "public class Client { void run() { Model model = null; } }",
        ),
    ];

    let (index, trees) = setup_java_test_graph(files);
    let resolver = JavaPlugin::new().expect("Failed to create JavaPlugin");

    let content = &trees[1].1;
    let tree = &trees[1].2;
    let pos = content.find("model = null").expect("find variable usage");
    let (line, col) = offset_to_point(content, pos);

    let resolution = resolver
        .resolve_at(tree, content, line, col, &index)
        .expect("resolve variable");
    let type_resolutions = resolver.resolve_type_of(&index, &resolution);

    assert_eq!(type_resolutions.len(), 1);
    let matches = resolver.find_matches(&index, &type_resolutions[0]);
    assert_eq!(matches.len(), 1);

    let idx = *index.fqn_map().get(&matches[0]).expect("node exists");
    assert_eq!(
        index.render_fqn(
            &index.topology()[idx],
            Some(&naviscope_java::naming::JavaNamingConvention)
        ),
        "Model"
    );
}

#[test]
fn given_method_symbol_when_goto_type_definition_then_returns_method_return_type() {
    let files = vec![
        ("Model.java", "public class Model {}"),
        (
            "Service.java",
            "public class Service { Model get() { return null; } }",
        ),
    ];

    let (index, trees) = setup_java_test_graph(files);
    let resolver = JavaPlugin::new().expect("Failed to create JavaPlugin");

    let content = &trees[1].1;
    let tree = &trees[1].2;
    let pos = content.find("get()").expect("find method symbol");
    let (line, col) = offset_to_point(content, pos);

    let resolution = resolver
        .resolve_at(tree, content, line, col, &index)
        .expect("resolve method");
    let type_resolutions = resolver.resolve_type_of(&index, &resolution);

    assert_eq!(type_resolutions.len(), 1);
    let matches = resolver.find_matches(&index, &type_resolutions[0]);
    assert_eq!(matches.len(), 1);

    let idx = *index.fqn_map().get(&matches[0]).expect("node exists");
    assert_eq!(
        index.render_fqn(
            &index.topology()[idx],
            Some(&naviscope_java::naming::JavaNamingConvention)
        ),
        "Model"
    );
}
