mod common;

use common::{offset_to_point, setup_java_test_graph};
use naviscope_core::features::CodeGraphLike;
use naviscope_java::JavaPlugin;
use naviscope_plugin::{SymbolQueryService, SymbolResolveService};

#[test]
fn given_interface_when_goto_implementation_then_returns_all_implementors() {
    let files = vec![
        ("IService.java", "public interface IService { void run(); }"),
        (
            "AService.java",
            "public class AService implements IService { public void run() {} }",
        ),
        (
            "BService.java",
            "public class BService implements IService { public void run() {} }",
        ),
    ];

    let (index, trees) = setup_java_test_graph(files);
    let resolver = JavaPlugin::new().expect("Failed to create JavaPlugin");

    let content = &trees[0].1;
    let tree = &trees[0].2;
    let pos = content.find("IService").expect("find interface");
    let (line, col) = offset_to_point(content, pos);

    let resolution = resolver
        .resolve_at(tree, content, line, col, &index)
        .expect("resolve interface");

    let impls = resolver.find_implementations(&index, &resolution);
    let mut fqns: Vec<String> = impls
        .iter()
        .map(|id| {
            let idx = *index.fqn_map().get(id).expect("node exists");
            index
                .render_fqn(
                    &index.topology()[idx],
                    Some(&naviscope_java::naming::JavaNamingConvention),
                )
                .to_string()
        })
        .collect();
    fqns.sort();

    assert_eq!(fqns, vec!["AService", "BService"]);
}

#[test]
fn given_interface_method_when_goto_implementation_then_returns_method_override() {
    let files = vec![
        ("IBase.java", "public interface IBase { void act(); }"),
        (
            "Impl.java",
            "public class Impl implements IBase { public void act() {} }",
        ),
    ];

    let (index, trees) = setup_java_test_graph(files);
    let resolver = JavaPlugin::new().expect("Failed to create JavaPlugin");

    let content = &trees[0].1;
    let tree = &trees[0].2;
    let pos = content.find("act()").expect("find interface method");
    let (line, col) = offset_to_point(content, pos);

    let resolution = resolver
        .resolve_at(tree, content, line, col, &index)
        .expect("resolve interface method");

    let impls = resolver.find_implementations(&index, &resolution);
    assert_eq!(impls.len(), 1);

    let idx = *index.fqn_map().get(&impls[0]).expect("node exists");
    assert_eq!(
        index.render_fqn(
            &index.topology()[idx],
            Some(&naviscope_java::naming::JavaNamingConvention)
        ),
        "Impl#act()"
    );
}
