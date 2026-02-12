//! Mock implementation of JavaTypeSystem for testing.
//! Moved from src/inference/adapters/mock.rs to tests/inference_mock_test.rs

use naviscope_api::models::TypeRef;
use std::collections::HashMap;

use naviscope_java::inference::{create_inference_context, infer_expression};
use naviscope_java::inference::JavaTypeSystem;
use naviscope_java::inference::{InheritanceProvider, MemberProvider, TypeProvider};
use naviscope_java::inference::{
    MemberInfo, MemberKind, TypeInfo, TypeKind, TypeResolutionContext,
};
use naviscope_java::inference::core::types::TypeParameter;
use naviscope_java::inference::scope::ScopeManager;
use naviscope_java::parser::JavaParser;

/// A mock type system for testing.
///
/// Can be built using a fluent API.
#[derive(Default)]
pub struct MockTypeSystem {
    types: HashMap<String, TypeInfo>,
    inheritance: HashMap<String, (Option<String>, Vec<String>)>, // (superclass, interfaces)
    members: HashMap<String, Vec<MemberInfo>>,
}

impl MockTypeSystem {
    /// Create a new empty mock.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a class to the mock.
    pub fn add_class(mut self, fqn: &str, super_class: Option<&str>) -> Self {
        self.types.insert(
            fqn.to_string(),
            TypeInfo {
                fqn: fqn.to_string(),
                kind: TypeKind::Class,
                modifiers: vec![],
                type_parameters: vec![],
            },
        );

        self.inheritance.insert(
            fqn.to_string(),
            (super_class.map(|s| s.to_string()), vec![]),
        );

        self
    }

    /// Add a class with generic type parameters.
    pub fn add_class_with_type_params(
        mut self,
        fqn: &str,
        super_class: Option<&str>,
        type_parameters: Vec<&str>,
    ) -> Self {
        self.types.insert(
            fqn.to_string(),
            TypeInfo {
                fqn: fqn.to_string(),
                kind: TypeKind::Class,
                modifiers: vec![],
                type_parameters: type_parameters
                    .into_iter()
                    .map(|name| TypeParameter {
                        name: name.to_string(),
                        bounds: vec![],
                    })
                    .collect(),
            },
        );

        self.inheritance.insert(
            fqn.to_string(),
            (super_class.map(|s| s.to_string()), vec![]),
        );

        self
    }

    /// Add an interface to the mock.
    pub fn add_interface(mut self, fqn: &str) -> Self {
        self.types.insert(
            fqn.to_string(),
            TypeInfo {
                fqn: fqn.to_string(),
                kind: TypeKind::Interface,
                modifiers: vec![],
                type_parameters: vec![],
            },
        );

        self.inheritance.insert(fqn.to_string(), (None, vec![]));

        self
    }

    /// Add an interface with generic type parameters.
    pub fn add_interface_with_type_params(
        mut self,
        fqn: &str,
        type_parameters: Vec<&str>,
    ) -> Self {
        self.types.insert(
            fqn.to_string(),
            TypeInfo {
                fqn: fqn.to_string(),
                kind: TypeKind::Interface,
                modifiers: vec![],
                type_parameters: type_parameters
                    .into_iter()
                    .map(|name| TypeParameter {
                        name: name.to_string(),
                        bounds: vec![],
                    })
                    .collect(),
            },
        );

        self.inheritance.insert(fqn.to_string(), (None, vec![]));

        self
    }

    /// Add interface implementation to a class.
    pub fn implements(mut self, class_fqn: &str, interface_fqn: &str) -> Self {
        if let Some((_super_class, interfaces)) = self.inheritance.get_mut(class_fqn) {
            interfaces.push(interface_fqn.to_string());
        } else {
            self.inheritance.insert(
                class_fqn.to_string(),
                (None, vec![interface_fqn.to_string()]),
            );
        }
        // Also ensure interface type exists
        if !self.types.contains_key(interface_fqn) {
            self = self.add_interface(interface_fqn);
        }
        self
    }

    /// Add a method to a class.
    pub fn add_method(mut self, class_fqn: &str, name: &str, return_type: TypeRef) -> Self {
        let member = MemberInfo {
            name: name.to_string(),
            fqn: format!("{}#{}", class_fqn, name),
            kind: MemberKind::Method,
            declaring_type: class_fqn.to_string(),
            type_ref: return_type,
            parameters: Some(vec![]),
            modifiers: vec![],
            generic_signature: None,
        };

        self.members
            .entry(class_fqn.to_string())
            .or_default()
            .push(member);

        self
    }

    /// Add a field to a class.
    pub fn add_field(mut self, class_fqn: &str, name: &str, field_type: TypeRef) -> Self {
        let member = MemberInfo {
            name: name.to_string(),
            fqn: format!("{}#{}", class_fqn, name),
            kind: MemberKind::Field,
            declaring_type: class_fqn.to_string(),
            type_ref: field_type,
            parameters: None,
            modifiers: vec![],
            generic_signature: None,
        };

        self.members
            .entry(class_fqn.to_string())
            .or_default()
            .push(member);

        self
    }
}

fn find_first_named<'a>(node: tree_sitter::Node<'a>, kind: &str) -> Option<tree_sitter::Node<'a>> {
    if node.kind() == kind {
        return Some(node);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(found) = find_first_named(child, kind) {
            return Some(found);
        }
    }
    None
}

impl TypeProvider for MockTypeSystem {
    fn get_type_info(&self, fqn: &str) -> Option<TypeInfo> {
        self.types.get(fqn).cloned()
    }

    fn resolve_type_name(&self, simple_name: &str, _ctx: &TypeResolutionContext) -> Option<String> {
        // Simple mock: just check if we have a type ending with simple name
        for fqn in self.types.keys() {
            if fqn.ends_with(&format!(".{}", simple_name)) || fqn == simple_name {
                return Some(fqn.clone());
            }
        }
        None
    }
}

impl InheritanceProvider for MockTypeSystem {
    fn get_superclass(&self, fqn: &str) -> Option<String> {
        self.inheritance
            .get(fqn)
            .and_then(|(super_class, _)| super_class.clone())
    }

    fn get_interfaces(&self, fqn: &str) -> Vec<String> {
        self.inheritance
            .get(fqn)
            .map(|(_, interfaces)| interfaces.clone())
            .unwrap_or_default()
    }

    fn get_direct_subtypes(&self, type_fqn: &str) -> Vec<String> {
        let mut subtypes = Vec::new();

        // Check inheritance map for both superclass and interfaces
        for (child, (super_class, interfaces)) in &self.inheritance {
            // Check if superclass matches
            if let Some(sc) = super_class {
                if sc == type_fqn {
                    subtypes.push(child.clone());
                }
            }

            // Check if any interface matches
            if interfaces.contains(&type_fqn.to_string()) {
                subtypes.push(child.clone());
            }
        }

        subtypes
    }

    fn walk_descendants(&self, type_fqn: &str) -> Box<dyn Iterator<Item = String>> {
        let mut descendants = Vec::new();
        let mut queue = std::collections::VecDeque::new();
        queue.push_back(type_fqn.to_string());

        let mut visited = std::collections::HashSet::new();
        visited.insert(type_fqn.to_string());

        while let Some(current) = queue.pop_front() {
            let direct = self.get_direct_subtypes(&current);
            for sub in direct {
                if visited.insert(sub.clone()) {
                    descendants.push(sub.clone());
                    queue.push_back(sub);
                }
            }
        }

        Box::new(descendants.into_iter())
    }

    fn walk_ancestors(&self, fqn: &str) -> Box<dyn Iterator<Item = String> + '_> {
        let mut result = vec![];
        let mut visited = std::collections::HashSet::new();
        let mut queue = std::collections::VecDeque::new();

        // Start with direct parents
        if let Some((super_class, interfaces)) = self.inheritance.get(fqn) {
            if let Some(s) = super_class {
                queue.push_back(s.clone());
            }
            for i in interfaces {
                queue.push_back(i.clone());
            }
        }

        while let Some(current) = queue.pop_front() {
            if visited.contains(&current) {
                continue;
            }
            visited.insert(current.clone());
            result.push(current.clone());

            // Add parents
            if let Some((super_class, interfaces)) = self.inheritance.get(&current) {
                if let Some(s) = super_class {
                    queue.push_back(s.clone());
                }
                for i in interfaces {
                    queue.push_back(i.clone());
                }
            }
        }

        Box::new(result.into_iter())
    }
}

impl MemberProvider for MockTypeSystem {
    fn get_members(&self, type_fqn: &str, member_name: &str) -> Vec<MemberInfo> {
        self.members
            .get(type_fqn)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|m| m.name == member_name)
            .collect()
    }

    fn get_all_members(&self, type_fqn: &str) -> Vec<MemberInfo> {
        self.members.get(type_fqn).cloned().unwrap_or_default()
    }
}

// =========================================================================
// Mock Construction Tests
// =========================================================================

#[test]
fn test_mock_add_class() {
    let ts = MockTypeSystem::new()
        .add_class("Parent", None)
        .add_class("Child", Some("Parent"));

    assert!(ts.get_type_info("Parent").is_some());
    assert!(ts.get_type_info("Child").is_some());
    assert_eq!(ts.get_superclass("Child"), Some("Parent".to_string()));
}

#[test]
fn test_mock_add_method() {
    let ts = MockTypeSystem::new().add_class("Foo", None).add_method(
        "Foo",
        "bar",
        TypeRef::Id("String".into()),
    );

    let members = ts.get_members("Foo", "bar");
    assert!(!members.is_empty());
    assert_eq!(members[0].name, "bar");
}

#[test]
fn test_mock_add_field() {
    let ts = MockTypeSystem::new().add_class("Foo", None).add_field(
        "Foo",
        "name",
        TypeRef::Id("String".into()),
    );

    let members = ts.get_members("Foo", "name");
    assert!(!members.is_empty());
    let member = &members[0];
    assert_eq!(member.name, "name");
    assert_eq!(member.kind, MemberKind::Field);
    assert_eq!(member.type_ref, TypeRef::Id("String".into()));
}

// =========================================================================
// Inheritance Hierarchy Tests
// =========================================================================

#[test]
fn test_find_inherited_member() {
    let ts = MockTypeSystem::new()
        .add_class("Parent", None)
        .add_method("Parent", "getContext", TypeRef::Id("Context".into()))
        .add_class("Child", Some("Parent"));

    // Child doesn't have getContext directly
    assert!(ts.get_members("Child", "getContext").is_empty());

    // But it should be found via hierarchy
    let members = ts.find_member_in_hierarchy("Child", "getContext");
    assert!(!members.is_empty());
    assert_eq!(members[0].declaring_type, "Parent");
}

#[test]
fn test_walk_ancestors() {
    let ts = MockTypeSystem::new()
        .add_class("Object", None)
        .add_class("Parent", Some("Object"))
        .add_class("Child", Some("Parent"));

    let ancestors: Vec<_> = ts.walk_ancestors("Child").collect();
    assert!(ancestors.contains(&"Parent".to_string()));
    assert!(ancestors.contains(&"Object".to_string()));
}

#[test]
fn test_interfaces() {
    let ts = MockTypeSystem::new()
        .add_class("MyList", None)
        .add_interface("List")
        .add_interface("Iterable")
        .implements("MyList", "List")
        .implements("List", "Iterable");

    let ifaces = ts.get_interfaces("MyList");
    assert!(ifaces.contains(&"List".to_string()));

    // Should walk to Iterable
    let ancestors: Vec<_> = ts.walk_ancestors("MyList").collect();
    assert!(ancestors.contains(&"List".to_string()));
    assert!(ancestors.contains(&"Iterable".to_string()));
}

// =========================================================================
// JavaTypeSystem Trait Tests (via MockTypeSystem)
// =========================================================================

#[test]
fn test_find_member_in_hierarchy_direct() {
    // Test that direct members are found first
    let ts = MockTypeSystem::new()
        .add_class("Base", None)
        .add_method("Base", "toString", TypeRef::Id("String".into()))
        .add_class("Derived", Some("Base"))
        .add_method("Derived", "toString", TypeRef::Id("StringOverride".into()));

    // Finding toString on Derived should return Derived's version
    let members = ts.find_member_in_hierarchy("Derived", "toString");
    assert!(!members.is_empty());
    let member = &members[0];
    assert_eq!(member.declaring_type, "Derived");
    assert_eq!(member.type_ref, TypeRef::Id("StringOverride".into()));
}

#[test]
fn test_find_member_in_hierarchy_inherited() {
    let ts = MockTypeSystem::new()
        .add_class("GrandParent", None)
        .add_method(
            "GrandParent",
            "legacyMethod",
            TypeRef::Id("LegacyType".into()),
        )
        .add_class("Parent", Some("GrandParent"))
        .add_class("Child", Some("Parent"));

    // Child should find GrandParent's method through hierarchy
    let members = ts.find_member_in_hierarchy("Child", "legacyMethod");
    assert!(!members.is_empty());
    let member = &members[0];
    assert_eq!(member.declaring_type, "GrandParent");
}

#[test]
fn test_find_member_in_interface_hierarchy() {
    let ts = MockTypeSystem::new()
        .add_interface("Iterator")
        .add_method("Iterator", "next", TypeRef::Id("Object".into()))
        .add_interface("ListIterator")
        .implements("ListIterator", "Iterator")
        .add_class("ConcreteIterator", None)
        .implements("ConcreteIterator", "ListIterator");

    // ConcreteIterator should find Iterator's method
    let members = ts.find_member_in_hierarchy("ConcreteIterator", "next");
    assert!(!members.is_empty());
    assert_eq!(members[0].declaring_type, "Iterator");
}

#[test]
fn test_find_nonexistent_member() {
    let ts = MockTypeSystem::new().add_class("Class", None);

    let members = ts.find_member_in_hierarchy("Class", "nonexistent");
    assert!(members.is_empty());
}

// =========================================================================
// Type Resolution Tests
// =========================================================================

#[test]
fn test_resolve_simple_type_name() {
    let ts = MockTypeSystem::new()
        .add_class("com.example.MyClass", None)
        .add_class("String", None);

    let ctx = TypeResolutionContext::default();

    // Simple name should match exact
    assert_eq!(
        ts.resolve_type_name("String", &ctx),
        Some("String".to_string())
    );

    // Should match types ending with the simple name
    assert_eq!(
        ts.resolve_type_name("MyClass", &ctx),
        Some("com.example.MyClass".to_string())
    );
}

// =========================================================================
// Field Access Chain Tests
// =========================================================================

#[test]
fn test_field_method_chain_types() {
    // Simulates: a.b.doB()
    // class A { B b; }
    // class B { void doB() {} }
    let ts = MockTypeSystem::new()
        .add_class("A", None)
        .add_field("A", "b", TypeRef::Id("B".into()))
        .add_class("B", None)
        .add_method("B", "doB", TypeRef::Raw("void".into()));

    // Verify field lookup
    let field_b = ts.find_member_in_hierarchy("A", "b");
    assert!(!field_b.is_empty());
    let field_b = &field_b[0];
    assert_eq!(field_b.type_ref, TypeRef::Id("B".into()));

    // Verify method lookup on B
    let method_dob = ts.find_member_in_hierarchy("B", "doB");
    assert!(!method_dob.is_empty());
    assert_eq!(method_dob[0].fqn, "B#doB");
}

#[test]
fn test_chained_method_calls() {
    // Simulates: response.getContext().get("key")
    // class HttpResponse { SessionContext getContext(); }
    // class SessionContext { Object get(String key); }
    let ts = MockTypeSystem::new()
        .add_class("HttpResponse", None)
        .add_method(
            "HttpResponse",
            "getContext",
            TypeRef::Id("SessionContext".into()),
        )
        .add_class("SessionContext", None)
        .add_method("SessionContext", "get", TypeRef::Id("Object".into()));

    // Simulate chain resolution:
    // 1. Find getContext on HttpResponse -> returns SessionContext
    let step1 = ts.find_member_in_hierarchy("HttpResponse", "getContext");
    assert!(!step1.is_empty());
    let context_type = step1[0].type_ref.clone();
    assert_eq!(context_type, TypeRef::Id("SessionContext".into()));

    // 2. Find get on SessionContext -> returns Object
    let type_fqn = match &context_type {
        TypeRef::Id(fqn) => fqn.clone(),
        _ => panic!("Expected Id type"),
    };
    let step2 = ts.find_member_in_hierarchy(&type_fqn, "get");
    assert!(!step2.is_empty());
    assert_eq!(step2[0].type_ref, TypeRef::Id("Object".into()));
}

// =========================================================================
// Complex Inheritance Tests
// =========================================================================

#[test]
fn test_diamond_inheritance() {
    // Test diamond inheritance pattern:
    //       Object
    //        / \
    //   Readable Appendable
    //        \ /
    //      Stream
    let ts = MockTypeSystem::new()
        .add_interface("Object")
        .add_method("Object", "toString", TypeRef::Id("String".into()))
        .add_interface("Readable")
        .add_method("Readable", "read", TypeRef::Raw("int".into()))
        .implements("Readable", "Object")
        .add_interface("Appendable")
        .add_method("Appendable", "append", TypeRef::Id("Appendable".into()))
        .implements("Appendable", "Object")
        .add_class("Stream", None)
        .implements("Stream", "Readable")
        .implements("Stream", "Appendable");

    // Stream should find all methods
    assert!(!ts.find_member_in_hierarchy("Stream", "read").is_empty());
    assert!(!ts.find_member_in_hierarchy("Stream", "append").is_empty());
    assert!(!ts.find_member_in_hierarchy("Stream", "toString").is_empty());
}

#[test]
fn test_spring_boot_pattern() {
    // Common Spring Boot pattern:
    // class MyController {
    //     private MyService service;
    //     void handle() { service.doWork(); }
    // }
    let ts = MockTypeSystem::new()
        .add_class("MyService", None)
        .add_method("MyService", "doWork", TypeRef::Raw("void".into()))
        .add_class("MyController", None)
        .add_field("MyController", "service", TypeRef::Id("MyService".into()));

    // From MyController, find service field
    let service_field = ts.find_member_in_hierarchy("MyController", "service");
    assert!(!service_field.is_empty());
    let service_type = service_field[0].type_ref.clone();
    assert_eq!(service_type, TypeRef::Id("MyService".into()));

    // From MyService, find doWork method
    let type_fqn = match &service_type {
        TypeRef::Id(fqn) => fqn.clone(),
        _ => panic!("Expected Id type"),
    };
    let do_work = ts.find_member_in_hierarchy(&type_fqn, "doWork");
    assert!(!do_work.is_empty());
}

// =========================================================================
// Generic Type Tests (Basic)
// =========================================================================

#[test]
fn test_generic_return_type() {
    // List<String>.get(int) -> String (simplified)
    // In reality this needs generic substitution, but we test the raw lookup
    let ts = MockTypeSystem::new()
        .add_interface("java.util.List")
        .add_method("java.util.List", "get", TypeRef::Id("E".into())) // E is type parameter
        .add_method("java.util.List", "size", TypeRef::Raw("int".into()));

    let get_method = ts.find_member_in_hierarchy("java.util.List", "get");
    assert!(!get_method.is_empty());
    // For now, returns the raw type parameter E
    assert_eq!(get_method[0].type_ref, TypeRef::Id("E".into()));
}

#[test]
fn test_infer_generic_method_call_with_substitution() {
    let source = r#"
import java.util.List;
class Demo {
    void run() {
        List<String> list = null;
        list.get(0);
    }
}
"#;

    let parser = JavaParser::new().expect("failed to create parser");
    let tree = parser.parse(source, None).expect("failed to parse source");
    let root = tree.root_node();
    let method_invocation = find_first_named(root, "method_invocation")
        .expect("expected method_invocation in test snippet");

    let ts = MockTypeSystem::new()
        .add_class("java.lang.String", None)
        .add_class("java.lang.Object", None)
        .add_interface_with_type_params("java.util.List", vec!["E"])
        .add_method("java.util.List", "get", TypeRef::Id("E".into()));

    let mut scope_manager = ScopeManager::new();
    let ctx = create_inference_context(
        &root,
        source,
        &ts,
        &mut scope_manager,
        None,
        vec!["java.util.List".to_string()],
    );

    let inferred = infer_expression(&method_invocation, &ctx);
    assert_eq!(inferred, Some(TypeRef::Id("java.lang.String".into())));
}

#[test]
fn test_infer_map_get_returns_integer() {
    let source = r#"
import java.util.Map;
class Demo {
    void run() {
        Map<String, Integer> map = null;
        map.get("k");
    }
}
"#;

    let parser = JavaParser::new().expect("failed to create parser");
    let tree = parser.parse(source, None).expect("failed to parse source");
    let root = tree.root_node();
    let method_invocation = find_first_named(root, "method_invocation")
        .expect("expected method_invocation in test snippet");

    let ts = MockTypeSystem::new()
        .add_class("java.lang.String", None)
        .add_class("java.lang.Integer", None)
        .add_interface_with_type_params("java.util.Map", vec!["K", "V"])
        .add_method("java.util.Map", "get", TypeRef::Id("V".into()));

    let mut scope_manager = ScopeManager::new();
    let ctx = create_inference_context(
        &root,
        source,
        &ts,
        &mut scope_manager,
        None,
        vec!["java.util.Map".to_string()],
    );

    let inferred = infer_expression(&method_invocation, &ctx);
    assert_eq!(inferred, Some(TypeRef::Id("java.lang.Integer".into())));
}

#[test]
fn test_infer_optional_get_returns_user() {
    let source = r#"
import java.util.Optional;
class User {}
class Demo {
    void run() {
        Optional<User> user = null;
        user.get();
    }
}
"#;

    let parser = JavaParser::new().expect("failed to create parser");
    let tree = parser.parse(source, None).expect("failed to parse source");
    let root = tree.root_node();
    let method_invocation = find_first_named(root, "method_invocation")
        .expect("expected method_invocation in test snippet");

    let ts = MockTypeSystem::new()
        .add_class("User", None)
        .add_interface_with_type_params("java.util.Optional", vec!["T"])
        .add_method("java.util.Optional", "get", TypeRef::Id("T".into()));

    let mut scope_manager = ScopeManager::new();
    let ctx = create_inference_context(
        &root,
        source,
        &ts,
        &mut scope_manager,
        None,
        vec!["java.util.Optional".to_string()],
    );

    let inferred = infer_expression(&method_invocation, &ctx);
    assert_eq!(inferred, Some(TypeRef::Id("User".into())));
}

#[test]
fn test_infer_nested_generic_map_get_returns_list_of_integer() {
    let source = r#"
import java.util.Map;
import java.util.List;
class Demo {
    void run() {
        Map<String, List<Integer>> map = null;
        map.get("k");
    }
}
"#;

    let parser = JavaParser::new().expect("failed to create parser");
    let tree = parser.parse(source, None).expect("failed to parse source");
    let root = tree.root_node();
    let method_invocation = find_first_named(root, "method_invocation")
        .expect("expected method_invocation in test snippet");

    let ts = MockTypeSystem::new()
        .add_class("java.lang.String", None)
        .add_class("java.lang.Integer", None)
        .add_interface_with_type_params("java.util.List", vec!["E"])
        .add_interface_with_type_params("java.util.Map", vec!["K", "V"])
        .add_method("java.util.Map", "get", TypeRef::Id("V".into()));

    let mut scope_manager = ScopeManager::new();
    let ctx = create_inference_context(
        &root,
        source,
        &ts,
        &mut scope_manager,
        None,
        vec!["java.util.Map".to_string(), "java.util.List".to_string()],
    );

    let inferred = infer_expression(&method_invocation, &ctx);
    assert_eq!(
        inferred,
        Some(TypeRef::Generic {
            base: Box::new(TypeRef::Id("java.util.List".into())),
            args: vec![TypeRef::Id("java.lang.Integer".into())],
        })
    );
}

#[test]
fn test_resolve_method_autoboxing_primitive_to_wrapper() {
    let ts = MockTypeSystem::new().add_class("java.lang.Integer", Some("java.lang.Object"));

    let candidates = vec![MemberInfo {
        name: "set".to_string(),
        fqn: "Demo#set".to_string(),
        kind: MemberKind::Method,
        declaring_type: "Demo".to_string(),
        type_ref: TypeRef::Raw("void".into()),
        parameters: Some(vec![naviscope_java::inference::ParameterInfo {
            name: "value".to_string(),
            type_ref: TypeRef::Id("java.lang.Integer".into()),
            is_varargs: false,
        }]),
        modifiers: vec![],
        generic_signature: None,
    }];

    let resolved = ts.resolve_method(&candidates, &[TypeRef::Raw("int".into())]);
    assert!(resolved.is_some());
}

#[test]
fn test_resolve_method_unboxing_with_widening() {
    let ts = MockTypeSystem::new().add_class("java.lang.Integer", Some("java.lang.Object"));

    let candidates = vec![MemberInfo {
        name: "set".to_string(),
        fqn: "Demo#set".to_string(),
        kind: MemberKind::Method,
        declaring_type: "Demo".to_string(),
        type_ref: TypeRef::Raw("void".into()),
        parameters: Some(vec![naviscope_java::inference::ParameterInfo {
            name: "value".to_string(),
            type_ref: TypeRef::Raw("long".into()),
            is_varargs: false,
        }]),
        modifiers: vec![],
        generic_signature: None,
    }];

    let resolved = ts.resolve_method(&candidates, &[TypeRef::Id("java.lang.Integer".into())]);
    assert!(resolved.is_some());
}

#[test]
fn test_resolve_method_varargs_expanded_arguments() {
    let ts = MockTypeSystem::new().add_class("java.lang.String", Some("java.lang.Object"));

    let candidates = vec![MemberInfo {
        name: "log".to_string(),
        fqn: "Demo#log".to_string(),
        kind: MemberKind::Method,
        declaring_type: "Demo".to_string(),
        type_ref: TypeRef::Raw("void".into()),
        parameters: Some(vec![
            naviscope_java::inference::ParameterInfo {
                name: "tag".to_string(),
                type_ref: TypeRef::Id("java.lang.String".into()),
                is_varargs: false,
            },
            naviscope_java::inference::ParameterInfo {
                name: "args".to_string(),
                type_ref: TypeRef::Array {
                    element: Box::new(TypeRef::Id("java.lang.String".into())),
                    dimensions: 1,
                },
                is_varargs: true,
            },
        ]),
        modifiers: vec![],
        generic_signature: None,
    }];

    let resolved = ts.resolve_method(
        &candidates,
        &[
            TypeRef::Id("java.lang.String".into()),
            TypeRef::Id("java.lang.String".into()),
            TypeRef::Id("java.lang.String".into()),
        ],
    );
    assert!(resolved.is_some());
}

#[test]
fn test_resolve_method_varargs_array_passthrough() {
    let ts = MockTypeSystem::new().add_class("java.lang.String", Some("java.lang.Object"));

    let candidates = vec![MemberInfo {
        name: "log".to_string(),
        fqn: "Demo#log".to_string(),
        kind: MemberKind::Method,
        declaring_type: "Demo".to_string(),
        type_ref: TypeRef::Raw("void".into()),
        parameters: Some(vec![
            naviscope_java::inference::ParameterInfo {
                name: "tag".to_string(),
                type_ref: TypeRef::Id("java.lang.String".into()),
                is_varargs: false,
            },
            naviscope_java::inference::ParameterInfo {
                name: "args".to_string(),
                type_ref: TypeRef::Array {
                    element: Box::new(TypeRef::Id("java.lang.String".into())),
                    dimensions: 1,
                },
                is_varargs: true,
            },
        ]),
        modifiers: vec![],
        generic_signature: None,
    }];

    let resolved = ts.resolve_method(
        &candidates,
        &[
            TypeRef::Id("java.lang.String".into()),
            TypeRef::Array {
                element: Box::new(TypeRef::Id("java.lang.String".into())),
                dimensions: 1,
            },
        ],
    );
    assert!(resolved.is_some());
}

#[test]
fn test_resolve_method_varargs_uses_explicit_marker() {
    let ts = MockTypeSystem::new().add_class("java.lang.String", Some("java.lang.Object"));

    let non_varargs = MemberInfo {
        name: "log".to_string(),
        fqn: "Demo#logArray".to_string(),
        kind: MemberKind::Method,
        declaring_type: "Demo".to_string(),
        type_ref: TypeRef::Raw("void".into()),
        parameters: Some(vec![
            naviscope_java::inference::ParameterInfo {
                name: "tag".to_string(),
                type_ref: TypeRef::Id("java.lang.String".into()),
                is_varargs: false,
            },
            naviscope_java::inference::ParameterInfo {
                name: "args".to_string(),
                type_ref: TypeRef::Array {
                    element: Box::new(TypeRef::Id("java.lang.String".into())),
                    dimensions: 1,
                },
                is_varargs: false,
            },
        ]),
        modifiers: vec![],
        generic_signature: None,
    };

    let varargs = MemberInfo {
        fqn: "Demo#logVarargs".to_string(),
        ..non_varargs.clone()
    };
    let varargs = MemberInfo {
        parameters: Some(vec![
            naviscope_java::inference::ParameterInfo {
                name: "tag".to_string(),
                type_ref: TypeRef::Id("java.lang.String".into()),
                is_varargs: false,
            },
            naviscope_java::inference::ParameterInfo {
                name: "args".to_string(),
                type_ref: TypeRef::Array {
                    element: Box::new(TypeRef::Id("java.lang.String".into())),
                    dimensions: 1,
                },
                is_varargs: true,
            },
        ]),
        ..varargs
    };

    let resolved = ts.resolve_method(
        &[non_varargs, varargs.clone()],
        &[
            TypeRef::Id("java.lang.String".into()),
            TypeRef::Id("java.lang.String".into()),
            TypeRef::Id("java.lang.String".into()),
        ],
    );

    assert_eq!(resolved.map(|m| m.fqn), Some("Demo#logVarargs".to_string()));
}
