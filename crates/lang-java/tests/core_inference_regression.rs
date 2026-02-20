use naviscope_api::models::TypeRef;
use naviscope_java::inference::core::subtyping::is_subtype;
use naviscope_java::inference::core::unification::{Substitution, unify};
use naviscope_java::inference::{
    InheritanceProvider, JavaTypeSystem, MemberInfo, MemberKind, MemberProvider, ParameterInfo,
    TypeInfo, TypeProvider, TypeResolutionContext,
};
use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Default)]
struct MiniTypeSystem {
    direct_parents: HashMap<String, Vec<String>>,
}

impl MiniTypeSystem {
    fn with_parent(mut self, ty: &str, parent: &str) -> Self {
        self.direct_parents
            .entry(ty.to_string())
            .or_default()
            .push(parent.to_string());
        self
    }
}

impl TypeProvider for MiniTypeSystem {
    fn get_type_info(&self, _fqn: &str) -> Option<TypeInfo> {
        None
    }

    fn resolve_type_name(
        &self,
        _simple_name: &str,
        _context: &TypeResolutionContext,
    ) -> Option<String> {
        None
    }
}

impl InheritanceProvider for MiniTypeSystem {
    fn get_superclass(&self, fqn: &str) -> Option<String> {
        self.direct_parents
            .get(fqn)
            .and_then(|p| p.first())
            .cloned()
    }

    fn get_interfaces(&self, fqn: &str) -> Vec<String> {
        self.direct_parents
            .get(fqn)
            .map(|p| p.iter().skip(1).cloned().collect())
            .unwrap_or_default()
    }

    fn walk_ancestors(&self, fqn: &str) -> Box<dyn Iterator<Item = String> + '_> {
        let mut queue: VecDeque<String> = VecDeque::new();
        let mut visited: HashSet<String> = HashSet::new();
        let mut out = Vec::new();

        if let Some(parents) = self.direct_parents.get(fqn) {
            for p in parents {
                queue.push_back(p.clone());
            }
        }

        while let Some(curr) = queue.pop_front() {
            if !visited.insert(curr.clone()) {
                continue;
            }
            out.push(curr.clone());
            if let Some(parents) = self.direct_parents.get(&curr) {
                for p in parents {
                    queue.push_back(p.clone());
                }
            }
        }

        Box::new(out.into_iter())
    }

    fn get_direct_subtypes(&self, fqn: &str) -> Vec<String> {
        self.direct_parents
            .iter()
            .filter(|(_, parents)| parents.iter().any(|p| p == fqn))
            .map(|(ty, _)| ty.clone())
            .collect()
    }

    fn walk_descendants(&self, fqn: &str) -> Box<dyn Iterator<Item = String> + '_> {
        Box::new(self.get_direct_subtypes(fqn).into_iter())
    }
}

impl MemberProvider for MiniTypeSystem {
    fn get_members(&self, _type_fqn: &str, _member_name: &str) -> Vec<MemberInfo> {
        vec![]
    }

    fn get_all_members(&self, _type_fqn: &str) -> Vec<MemberInfo> {
        vec![]
    }
}

fn method(fqn: &str, param: TypeRef) -> MemberInfo {
    MemberInfo {
        name: "set".to_string(),
        fqn: fqn.to_string(),
        kind: MemberKind::Method,
        declaring_type: "Demo".to_string(),
        type_ref: TypeRef::Raw("void".to_string()),
        parameters: Some(vec![ParameterInfo {
            name: "value".to_string(),
            type_ref: param,
            is_varargs: false,
        }]),
        modifiers: vec![],
        generic_signature: None,
    }
}

#[test]
fn resolve_method_prefers_exact_match_over_wider_match() {
    let ts = MiniTypeSystem::default()
        .with_parent("java.lang.Integer", "java.lang.Number")
        .with_parent("java.lang.Number", "java.lang.Object");

    let candidates = vec![
        method("Demo#setNumber", TypeRef::Id("java.lang.Number".to_string())),
        method("Demo#setInteger", TypeRef::Id("java.lang.Integer".to_string())),
    ];

    let resolved = ts
        .resolve_method(&candidates, &[TypeRef::Id("java.lang.Integer".to_string())])
        .expect("resolved");
    assert_eq!(resolved.fqn, "Demo#setInteger");
}

#[test]
fn subtyping_supports_boxing_and_reference_widening() {
    let ts = MiniTypeSystem::default()
        .with_parent("java.lang.Integer", "java.lang.Number")
        .with_parent("java.lang.Number", "java.lang.Object");

    assert!(is_subtype(
        &TypeRef::Raw("int".to_string()),
        &TypeRef::Id("java.lang.Number".to_string()),
        &ts
    ));
}

#[test]
fn unify_binds_generic_parameter_in_nested_type() {
    let lhs = TypeRef::Generic {
        base: Box::new(TypeRef::Id("java.util.List".to_string())),
        args: vec![TypeRef::Id("T".to_string())],
    };
    let rhs = TypeRef::Generic {
        base: Box::new(TypeRef::Id("java.util.List".to_string())),
        args: vec![TypeRef::Id("java.lang.String".to_string())],
    };

    let subst = unify(&lhs, &rhs).expect("unify");
    let applied = subst.apply(&TypeRef::Id("T".to_string()));
    assert_eq!(applied, TypeRef::Id("java.lang.String".to_string()));
}

#[test]
fn unify_rejects_conflicting_type_variable_bindings() {
    let lhs = TypeRef::Generic {
        base: Box::new(TypeRef::Id("Pair".to_string())),
        args: vec![TypeRef::Id("T".to_string()), TypeRef::Id("T".to_string())],
    };
    let rhs = TypeRef::Generic {
        base: Box::new(TypeRef::Id("Pair".to_string())),
        args: vec![
            TypeRef::Id("java.lang.String".to_string()),
            TypeRef::Id("java.lang.Integer".to_string()),
        ],
    };

    assert!(unify(&lhs, &rhs).is_none());
}

#[test]
fn substitution_applies_inside_wildcard_bounds() {
    let mut subst = Substitution::new();
    subst.insert("E".to_string(), TypeRef::Id("com.A".to_string()));

    let ty = TypeRef::Wildcard {
        bound: Some(Box::new(TypeRef::Id("E".to_string()))),
        is_upper_bound: false,
    };

    let applied = subst.apply(&ty);
    assert_eq!(
        applied,
        TypeRef::Wildcard {
            bound: Some(Box::new(TypeRef::Id("com.A".to_string()))),
            is_upper_bound: false,
        }
    );
}
