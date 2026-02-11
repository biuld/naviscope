mod common;

use common::{offset_to_point, setup_java_test_graph};
use naviscope_core::features::CodeGraphLike;
use naviscope_java::JavaPlugin;
use naviscope_plugin::{SymbolQueryService, SymbolResolveService};

#[test]
fn test_goto_implementation_interface() {
    let files = vec![
        ("IBase.java", "public interface IBase { void act(); }"),
        (
            "ImplA.java",
            "public class ImplA implements IBase { public void act() {} }",
        ),
        (
            "ImplB.java",
            "public class ImplB implements IBase { public void act() {} }",
        ),
    ];
    let (index, trees) = setup_java_test_graph(files);
    let resolver = JavaPlugin::new().expect("Failed to create JavaPlugin");

    let base_content = &trees[0].1;
    let base_tree = &trees[0].2;

    // Resolve 'IBase'
    let usage_pos = base_content.find("IBase").unwrap();
    let (line, col) = offset_to_point(base_content, usage_pos);
    let res = resolver
        .resolve_at(base_tree, base_content, line, col, &index)
        .expect("Should resolve IBase");

    let impls = resolver.find_implementations(&index, &res);
    assert_eq!(impls.len(), 2);

    let fqns: Vec<_> = impls
        .iter()
        .map(|&id| {
            let idx = *index.fqn_map().get(&id).expect("Node not found");
            index
                .render_fqn(
                    &index.topology()[idx],
                    Some(&naviscope_java::naming::JavaNamingConvention),
                )
                .to_string()
        })
        .collect();
    assert!(fqns.contains(&"ImplA".to_string()));
    assert!(fqns.contains(&"ImplB".to_string()));
}

#[test]
fn test_goto_implementation_method() {
    let files = vec![
        ("IBase.java", "public interface IBase { void act(); }"),
        (
            "Impl.java",
            "public class Impl implements IBase { public void act() {} }",
        ),
    ];
    let (index, trees) = setup_java_test_graph(files);
    let resolver = JavaPlugin::new().expect("Failed to create JavaPlugin");

    let base_content = &trees[0].1;
    let base_tree = &trees[0].2;

    // Resolve 'act' in IBase
    let usage_pos = base_content.find("act()").unwrap();
    let (line, col) = offset_to_point(base_content, usage_pos);
    let res = resolver
        .resolve_at(base_tree, base_content, line, col, &index)
        .expect("Should resolve act");

    let impls = resolver.find_implementations(&index, &res);
    assert_eq!(impls.len(), 1);
    let idx = *index.fqn_map().get(&impls[0]).expect("Node not found");
    assert_eq!(
        index.render_fqn(
            &index.topology()[idx],
            Some(&naviscope_java::naming::JavaNamingConvention)
        ),
        "Impl#act"
    );
}
