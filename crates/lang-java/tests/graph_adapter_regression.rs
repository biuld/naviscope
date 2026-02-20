mod common;

use common::setup_java_test_graph;
use lasso::Key;
use naviscope_api::models::fqn::FqnNode;
use naviscope_api::models::graph::{EdgeType, GraphNode, NodeKind};
use naviscope_api::models::symbol::{FqnId, FqnReader, Symbol};
use naviscope_java::inference::adapters::CodeGraphTypeSystem;
use naviscope_java::inference::{
    InheritanceProvider, MemberProvider, TypeProvider, TypeResolutionContext,
};
use naviscope_plugin::{CodeGraph, Direction};
use std::collections::HashMap;
use std::path::Path;

#[test]
fn resolve_type_name_prefers_explicit_import() {
    let files = vec![
        ("src/a/Foo.java", "package a; public class Foo {}"),
        ("src/b/Foo.java", "package b; public class Foo {}"),
        (
            "src/t/Use.java",
            "package t; import a.Foo; import b.*; class Use { Foo foo; }",
        ),
    ];
    let (graph, _) = setup_java_test_graph(files);
    let ts = CodeGraphTypeSystem::new(&graph);
    let ctx = TypeResolutionContext {
        package: Some("t".to_string()),
        imports: vec!["a.Foo".to_string(), "b.*".to_string()],
        ..TypeResolutionContext::default()
    };

    assert_eq!(ts.resolve_type_name("Foo", &ctx), Some("a.Foo".to_string()));
}

#[test]
fn resolve_type_name_prefers_same_package_before_wildcard_import() {
    let files = vec![
        ("src/t/Foo.java", "package t; public class Foo {}"),
        ("src/b/Foo.java", "package b; public class Foo {}"),
        (
            "src/t/Use.java",
            "package t; import b.*; class Use { Foo foo; }",
        ),
    ];
    let (graph, _) = setup_java_test_graph(files);
    let ts = CodeGraphTypeSystem::new(&graph);
    let ctx = TypeResolutionContext {
        package: Some("t".to_string()),
        imports: vec!["b.*".to_string()],
        ..TypeResolutionContext::default()
    };

    assert_eq!(ts.resolve_type_name("Foo", &ctx), Some("t.Foo".to_string()));
}

#[test]
fn walk_ancestors_and_descendants_respect_max_depth() {
    let files = vec![(
        "src/p/Chain.java",
        r#"
package p;
class C0 {}
class C1 extends C0 {}
class C2 extends C1 {}
class C3 extends C2 {}
class C4 extends C3 {}
class C5 extends C4 {}
class C6 extends C5 {}
class C7 extends C6 {}
class C8 extends C7 {}
class C9 extends C8 {}
class C10 extends C9 {}
class C11 extends C10 {}
"#,
    )];

    let (graph, _) = setup_java_test_graph(files);
    let ts = CodeGraphTypeSystem::new(&graph);

    let ancestors: Vec<_> = ts.walk_ancestors("p.C11").collect();
    assert_eq!(ancestors.len(), 10);
    assert!(ancestors.contains(&"p.C1".to_string()));
    assert!(!ancestors.contains(&"p.C0".to_string()));

    let descendants: Vec<_> = ts.walk_descendants("p.C0").collect();
    assert_eq!(descendants.len(), 10);
    assert!(descendants.contains(&"p.C10".to_string()));
    assert!(!descendants.contains(&"p.C11".to_string()));
}

#[test]
fn type_info_and_members_preserve_modifiers() {
    let files = vec![(
        "src/p/User.java",
        "package p; public class User { public static final String NAME = \"x\"; public void ping() {} }",
    )];

    let (graph, _) = setup_java_test_graph(files);
    let ts = CodeGraphTypeSystem::new(&graph);

    let info = ts.get_type_info("p.User").expect("type info");
    assert!(info.modifiers.iter().any(|m| m == "public"));

    let methods = ts.get_members("p.User", "ping");
    assert!(!methods.is_empty());
    assert!(
        methods
            .iter()
            .any(|m| m.modifiers.iter().any(|modi| modi == "public"))
    );
}

#[test]
fn method_parameters_keep_original_names_from_graph_metadata() {
    let files = vec![(
        "src/p/User.java",
        "package p; public class User { public void greet(String name, int age) {} }",
    )];

    let (graph, _) = setup_java_test_graph(files);
    let ts = CodeGraphTypeSystem::new(&graph);

    let method = ts
        .get_members("p.User", "greet")
        .into_iter()
        .next()
        .expect("greet member");
    let params = method.parameters.expect("method params");
    assert_eq!(params.len(), 2);
    assert_eq!(params[0].name, "name");
    assert_eq!(params[1].name, "age");
}

struct FakeGraph {
    fqns: HashMap<String, Vec<FqnId>>,
    nodes: HashMap<FqnId, FqnNode>,
    neighbors: HashMap<(FqnId, u8, Option<EdgeType>), Vec<FqnId>>,
    atoms: HashMap<u32, String>,
}

impl FakeGraph {
    fn new() -> Self {
        Self {
            fqns: HashMap::new(),
            nodes: HashMap::new(),
            neighbors: HashMap::new(),
            atoms: HashMap::new(),
        }
    }
}

impl FqnReader for FakeGraph {
    fn resolve_node(&self, id: FqnId) -> Option<FqnNode> {
        self.nodes.get(&id).cloned()
    }

    fn resolve_atom(&self, atom: Symbol) -> &str {
        self.atoms
            .get(&(atom.0.into_usize() as u32))
            .map(String::as_str)
            .unwrap_or("")
    }
}

impl CodeGraph for FakeGraph {
    fn resolve_fqn(&self, fqn: &str) -> Vec<FqnId> {
        self.fqns.get(fqn).cloned().unwrap_or_default()
    }

    fn get_node_at(&self, _path: &Path, _line: usize, _col: usize) -> Option<FqnId> {
        None
    }

    fn resolve_atom(&self, atom: Symbol) -> &str {
        <Self as FqnReader>::resolve_atom(self, atom)
    }

    fn fqns(&self) -> &dyn FqnReader {
        self
    }

    fn get_node(&self, _id: FqnId) -> Option<GraphNode> {
        None
    }

    fn get_neighbors(
        &self,
        id: FqnId,
        direction: Direction,
        edge_type: Option<EdgeType>,
    ) -> Vec<FqnId> {
        self.neighbors
            .get(&(id, dir_tag(direction), edge_type))
            .cloned()
            .unwrap_or_default()
    }
}

fn atom(id: u32) -> Symbol {
    Symbol(lasso::Spur::try_from_usize(id as usize).expect("spur"))
}

fn dir_tag(direction: Direction) -> u8 {
    match direction {
        Direction::Incoming => 0,
        Direction::Outgoing => 1,
    }
}

fn fake_graph_for_dedup() -> FakeGraph {
    let mut g = FakeGraph::new();

    // atoms
    g.atoms.insert(1, "p".to_string());
    g.atoms.insert(2, "A".to_string());
    g.atoms.insert(3, "B".to_string());
    g.atoms.insert(4, "I".to_string());

    // FQN tree:
    // p          (100)
    // ├─ A       (101) and duplicate A (102)
    // ├─ B       (103)
    // └─ I       (104) and duplicate I (105)
    g.nodes.insert(
        FqnId(100),
        FqnNode {
            parent: None,
            name: atom(1),
            kind: NodeKind::Package,
        },
    );
    g.nodes.insert(
        FqnId(101),
        FqnNode {
            parent: Some(FqnId(100)),
            name: atom(2),
            kind: NodeKind::Class,
        },
    );
    g.nodes.insert(
        FqnId(102),
        FqnNode {
            parent: Some(FqnId(100)),
            name: atom(2),
            kind: NodeKind::Class,
        },
    );
    g.nodes.insert(
        FqnId(103),
        FqnNode {
            parent: Some(FqnId(100)),
            name: atom(3),
            kind: NodeKind::Class,
        },
    );
    g.nodes.insert(
        FqnId(104),
        FqnNode {
            parent: Some(FqnId(100)),
            name: atom(4),
            kind: NodeKind::Interface,
        },
    );
    g.nodes.insert(
        FqnId(105),
        FqnNode {
            parent: Some(FqnId(100)),
            name: atom(4),
            kind: NodeKind::Interface,
        },
    );

    g.fqns.insert("p.A".to_string(), vec![FqnId(101), FqnId(102)]);
    g.fqns.insert("p.I".to_string(), vec![FqnId(104), FqnId(105)]);

    // get_interfaces("p.A"): duplicates from duplicate node ids and repeated neighbors
    g.neighbors.insert(
        (FqnId(101), dir_tag(Direction::Outgoing), Some(EdgeType::Implements)),
        vec![FqnId(104), FqnId(104)],
    );
    g.neighbors.insert(
        (FqnId(102), dir_tag(Direction::Outgoing), Some(EdgeType::Implements)),
        vec![FqnId(104), FqnId(105)],
    );

    // get_direct_subtypes("p.I"): duplicates across InheritsFrom/Implements and duplicate I ids
    g.neighbors.insert(
        (
            FqnId(104),
            dir_tag(Direction::Incoming),
            Some(EdgeType::InheritsFrom),
        ),
        vec![FqnId(101)],
    );
    g.neighbors.insert(
        (
            FqnId(104),
            dir_tag(Direction::Incoming),
            Some(EdgeType::Implements),
        ),
        vec![FqnId(101), FqnId(103), FqnId(101)],
    );
    g.neighbors.insert(
        (
            FqnId(105),
            dir_tag(Direction::Incoming),
            Some(EdgeType::InheritsFrom),
        ),
        vec![FqnId(103)],
    );
    g.neighbors.insert(
        (
            FqnId(105),
            dir_tag(Direction::Incoming),
            Some(EdgeType::Implements),
        ),
        vec![FqnId(103)],
    );

    g
}

#[test]
fn get_interfaces_returns_unique_fqns() {
    let graph = fake_graph_for_dedup();
    let ts = CodeGraphTypeSystem::new(&graph);

    assert_eq!(ts.get_interfaces("p.A"), vec!["p.I".to_string()]);
}

#[test]
fn get_direct_subtypes_returns_unique_fqns() {
    let graph = fake_graph_for_dedup();
    let ts = CodeGraphTypeSystem::new(&graph);

    assert_eq!(
        ts.get_direct_subtypes("p.I"),
        vec!["p.A".to_string(), "p.B".to_string()]
    );
}
