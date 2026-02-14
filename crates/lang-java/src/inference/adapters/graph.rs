//! CodeGraph adapter for JavaTypeSystem.
//!
//! Adapts the CodeGraph to the JavaTypeSystem trait.

use lasso::Key;
use naviscope_api::models::TypeRef;
use naviscope_api::models::graph::{EdgeType, NodeKind, NodeMetadata};
use naviscope_api::models::symbol::{FqnId, Symbol};
use naviscope_plugin::{CodeGraph, Direction};
use std::collections::HashSet;
use std::sync::Arc;

use crate::inference::core::types::TypeParameter;
use crate::inference::{InheritanceProvider, MemberProvider, TypeProvider};
use crate::inference::{MemberInfo, MemberKind, TypeInfo, TypeKind, TypeResolutionContext};
use crate::model::{JavaIndexMetadata, JavaNodeMetadata};

/// Adapter that implements JavaTypeSystem using CodeGraph.
pub struct CodeGraphTypeSystem<'a> {
    graph: &'a dyn CodeGraph,
}

impl<'a> CodeGraphTypeSystem<'a> {
    /// Create a new adapter wrapping the given CodeGraph.
    pub fn new(graph: &'a dyn CodeGraph) -> Self {
        Self { graph }
    }

    /// Convert NodeKind to TypeKind.
    fn node_kind_to_type_kind(&self, kind: &NodeKind) -> TypeKind {
        match kind {
            NodeKind::Class => TypeKind::Class,
            NodeKind::Interface => TypeKind::Interface,
            NodeKind::Enum => TypeKind::Enum,
            NodeKind::Annotation => TypeKind::Annotation,
            _ => TypeKind::Class,
        }
    }

    fn resolve_sid(&self, sid: u32) -> Option<String> {
        lasso::Spur::try_from_usize(sid as usize)
            .map(|spur| self.graph.fqns().resolve_atom(Symbol(spur)).to_string())
    }

    /// Extract modifiers from node metadata.
    fn extract_modifiers(&self, metadata: &Arc<dyn NodeMetadata>) -> Vec<String> {
        if let Some(java_meta) = metadata.as_any().downcast_ref::<JavaNodeMetadata>() {
            return match java_meta {
                JavaNodeMetadata::Class { modifiers_sids, .. }
                | JavaNodeMetadata::Interface { modifiers_sids, .. }
                | JavaNodeMetadata::Enum { modifiers_sids, .. }
                | JavaNodeMetadata::Annotation { modifiers_sids }
                | JavaNodeMetadata::Method { modifiers_sids, .. }
                | JavaNodeMetadata::Field { modifiers_sids, .. } => modifiers_sids
                    .iter()
                    .filter_map(|sid| self.resolve_sid(*sid))
                    .collect(),
                _ => vec![],
            };
        }

        if let Some(java_meta) = metadata.as_any().downcast_ref::<JavaIndexMetadata>() {
            return match java_meta {
                JavaIndexMetadata::Class { modifiers, .. } => modifiers.clone(),
                JavaIndexMetadata::Interface { modifiers, .. } => modifiers.clone(),
                JavaIndexMetadata::Enum { modifiers, .. } => modifiers.clone(),
                JavaIndexMetadata::Method { modifiers, .. } => modifiers.clone(),
                JavaIndexMetadata::Field { modifiers, .. } => modifiers.clone(),
                _ => vec![],
            };
        }
        vec![]
    }

    fn extract_type_parameters(&self, metadata: &Arc<dyn NodeMetadata>) -> Vec<TypeParameter> {
        if let Some(java_meta) = metadata.as_any().downcast_ref::<JavaNodeMetadata>() {
            let names: Vec<String> = match java_meta {
                JavaNodeMetadata::Class {
                    type_parameters_sids,
                    ..
                }
                | JavaNodeMetadata::Interface {
                    type_parameters_sids,
                    ..
                } => type_parameters_sids
                    .iter()
                    .filter_map(|sid| self.resolve_sid(*sid))
                    .collect(),
                _ => vec![],
            };
            return names
                .into_iter()
                .map(|name| TypeParameter {
                    name,
                    bounds: vec![],
                })
                .collect();
        }

        if let Some(java_meta) = metadata.as_any().downcast_ref::<JavaIndexMetadata>() {
            let names: Vec<String> = match java_meta {
                JavaIndexMetadata::Class {
                    type_parameters, ..
                }
                | JavaIndexMetadata::Interface {
                    type_parameters, ..
                } => type_parameters.clone(),
                _ => vec![],
            };
            return names
                .into_iter()
                .map(|name| TypeParameter {
                    name,
                    bounds: vec![],
                })
                .collect();
        }

        vec![]
    }

    /// Extract type ref from metadata.
    fn extract_type_from_metadata(&self, metadata: &Arc<dyn NodeMetadata>) -> TypeRef {
        if let Some(java_meta) = metadata.as_any().downcast_ref::<JavaNodeMetadata>() {
            return match java_meta {
                JavaNodeMetadata::Method { return_type, .. } => return_type.clone(),
                JavaNodeMetadata::Field { type_ref, .. } => type_ref.clone(),
                _ => TypeRef::Id("java.lang.Object".to_string()),
            };
        }

        if let Some(java_meta) = metadata.as_any().downcast_ref::<JavaIndexMetadata>() {
            return match java_meta {
                JavaIndexMetadata::Method { return_type, .. } => return_type.clone(),
                JavaIndexMetadata::Field { type_ref, .. } => type_ref.clone(),
                _ => TypeRef::Id("java.lang.Object".to_string()),
            };
        }
        TypeRef::Id("java.lang.Object".to_string())
    }

    /// Render FQN for a node ID.
    fn render_fqn_id(&self, node_id: FqnId) -> String {
        use crate::naming::JavaNamingConvention;
        use naviscope_plugin::NamingConvention;

        // Use Java naming convention to render FQN
        JavaNamingConvention::default().render_fqn(node_id, self.graph.fqns())
    }

    /// Extract parameters from metadata.
    fn extract_parameters(
        &self,
        metadata: &Arc<dyn NodeMetadata>,
    ) -> Option<Vec<crate::inference::ParameterInfo>> {
        use crate::inference::ParameterInfo;

        if let Some(java_meta) = metadata.as_any().downcast_ref::<JavaNodeMetadata>() {
            match java_meta {
                JavaNodeMetadata::Method { parameters, .. } => {
                    return Some(
                        parameters
                            .iter()
                            .enumerate()
                            .map(|(i, p)| ParameterInfo {
                                name: self
                                    .resolve_sid(p.name_sid)
                                    .unwrap_or_else(|| format!("arg{}", i)),
                                type_ref: p.type_ref.clone(),
                                is_varargs: p.is_varargs,
                            })
                            .collect(),
                    );
                }
                _ => return None,
            }
        }

        if let Some(java_meta) = metadata.as_any().downcast_ref::<JavaIndexMetadata>() {
            match java_meta {
                JavaIndexMetadata::Method { parameters, .. } => {
                    return Some(
                        parameters
                            .iter()
                            .map(|p| ParameterInfo {
                                name: p.name.clone(),
                                type_ref: p.type_ref.clone(),
                                is_varargs: p.is_varargs,
                            })
                            .collect(),
                    );
                }
                _ => return None,
            }
        }
        None
    }
}

impl<'a> TypeProvider for CodeGraphTypeSystem<'a> {
    fn get_type_info(&self, fqn: &str) -> Option<TypeInfo> {
        let node_ids = self.graph.resolve_fqn(fqn);
        let node_id = *node_ids.first()?;
        let node = self.graph.get_node(node_id)?;

        Some(TypeInfo {
            fqn: fqn.to_string(),
            kind: self.node_kind_to_type_kind(&node.kind),
            modifiers: self.extract_modifiers(&node.metadata),
            type_parameters: self.extract_type_parameters(&node.metadata),
        })
    }

    fn resolve_type_name(&self, simple_name: &str, ctx: &TypeResolutionContext) -> Option<String> {
        // 0. Check known FQNs (Current file types)
        for fqn in &ctx.known_fqns {
            if fqn.ends_with(&format!(".{}", simple_name))
                || fqn.ends_with(&format!("#{}", simple_name))
            {
                return Some(fqn.clone());
            }
        }

        // 1. Check explicit imports
        for imp in &ctx.imports {
            if imp.ends_with(&format!(".{}", simple_name)) {
                return Some(imp.clone());
            }
        }

        // 2. Check same package
        if let Some(pkg) = &ctx.package {
            let candidate = format!("{}.{}", pkg, simple_name);
            if !self.graph.resolve_fqn(&candidate).is_empty() {
                return Some(candidate);
            }
        }

        // 3. Check wildcard imports
        for imp in &ctx.imports {
            if imp.ends_with(".*") {
                let prefix = &imp[..imp.len() - 2];
                let candidate = format!("{}.{}", prefix, simple_name);
                if !self.graph.resolve_fqn(&candidate).is_empty() {
                    return Some(candidate);
                }
            }
        }

        // 4. Check java.lang
        let java_lang = format!("java.lang.{}", simple_name);
        if !self.graph.resolve_fqn(&java_lang).is_empty() {
            return Some(java_lang);
        }

        // 5. Fallback to raw name if it exists in graph (Default Package)
        let results = self.graph.resolve_fqn(simple_name);
        if !results.is_empty() {
            return Some(simple_name.to_string());
        }

        None
    }
}

impl<'a> InheritanceProvider for CodeGraphTypeSystem<'a> {
    fn get_superclass(&self, fqn: &str) -> Option<String> {
        let node_ids = self.graph.resolve_fqn(fqn);

        for node_id in node_ids {
            let neighbors = self.graph.get_neighbors(
                node_id,
                Direction::Outgoing,
                Some(EdgeType::InheritsFrom),
            );

            if let Some(&parent_id) = neighbors.first() {
                return Some(self.render_fqn_id(parent_id));
            }
        }

        None
    }

    fn get_interfaces(&self, fqn: &str) -> Vec<String> {
        let node_ids = self.graph.resolve_fqn(fqn);
        let mut seen = HashSet::new();
        let mut result = vec![];

        for node_id in node_ids {
            let neighbors =
                self.graph
                    .get_neighbors(node_id, Direction::Outgoing, Some(EdgeType::Implements));

            for iface_id in neighbors {
                let iface_fqn = self.render_fqn_id(iface_id);
                if seen.insert(iface_fqn.clone()) {
                    result.push(iface_fqn);
                }
            }
        }

        result
    }

    fn walk_ancestors(&self, fqn: &str) -> Box<dyn Iterator<Item = String> + '_> {
        Box::new(AncestorIterator::new(self, fqn, 10))
    }

    fn get_direct_subtypes(&self, fqn: &str) -> Vec<String> {
        let node_ids = self.graph.resolve_fqn(fqn);
        let mut seen = HashSet::new();
        let mut result = vec![];

        for node_id in node_ids {
            // Find nodes that inherit from or implement this type (Incoming edges)
            let subs = self.graph.get_neighbors(
                node_id,
                Direction::Incoming,
                Some(EdgeType::InheritsFrom),
            );
            for sub_id in subs {
                let sub_fqn = self.render_fqn_id(sub_id);
                if seen.insert(sub_fqn.clone()) {
                    result.push(sub_fqn);
                }
            }

            let impls =
                self.graph
                    .get_neighbors(node_id, Direction::Incoming, Some(EdgeType::Implements));
            for sub_id in impls {
                let sub_fqn = self.render_fqn_id(sub_id);
                if seen.insert(sub_fqn.clone()) {
                    result.push(sub_fqn);
                }
            }
        }

        result
    }

    fn walk_descendants(&self, fqn: &str) -> Box<dyn Iterator<Item = String> + '_> {
        Box::new(DescendantIterator::new(self, fqn, 10))
    }
}

impl<'a> MemberProvider for CodeGraphTypeSystem<'a> {
    fn get_members(&self, type_fqn: &str, member_name: &str) -> Vec<MemberInfo> {
        // With signature-based FQNs, methods are stored as e.g. `A#target(int)`.
        // We can't construct the full member FQN from just the simple name, so
        // we traverse the type's children and match by simple name.
        // Normalize: callers may pass either `leaf` or `leaf()` as the member name.
        let needle = crate::naming::extract_simple_name(member_name);
        let node_ids = self.graph.resolve_fqn(type_fqn);
        let mut members = Vec::new();

        for &type_node_id in &node_ids {
            let children = self.graph.get_neighbors(
                type_node_id,
                Direction::Outgoing,
                Some(EdgeType::Contains),
            );

            for child_id in children {
                let Some(node) = self.graph.get_node(child_id) else {
                    continue;
                };

                let kind = match &node.kind {
                    NodeKind::Method => MemberKind::Method,
                    NodeKind::Field => MemberKind::Field,
                    NodeKind::Constructor => MemberKind::Constructor,
                    _ => continue,
                };

                let child_fqn = self.render_fqn_id(child_id);
                // Extract the member part (after `#`) and strip signature to compare
                let raw_member = crate::naming::extract_member_name(&child_fqn)
                    .unwrap_or_else(|| self.graph.fqns().resolve_atom(node.name));
                let simple = crate::naming::extract_simple_name(raw_member);

                if simple != needle {
                    continue;
                }

                let type_ref = self.extract_type_from_metadata(&node.metadata);

                members.push(MemberInfo {
                    name: raw_member.to_string(),
                    fqn: child_fqn,
                    kind,
                    declaring_type: type_fqn.to_string(),
                    type_ref,
                    parameters: self.extract_parameters(&node.metadata),
                    modifiers: self.extract_modifiers(&node.metadata),
                    generic_signature: None,
                });
            }
        }

        members
    }

    fn get_all_members(&self, type_fqn: &str) -> Vec<MemberInfo> {
        let node_ids = self.graph.resolve_fqn(type_fqn);
        let mut members = Vec::new();

        if let Some(&node_id) = node_ids.first() {
            let children =
                self.graph
                    .get_neighbors(node_id, Direction::Outgoing, Some(EdgeType::Contains));

            for child_id in children {
                if let Some(node) = self.graph.get_node(child_id) {
                    let kind = match &node.kind {
                        NodeKind::Method => MemberKind::Method,
                        NodeKind::Field => MemberKind::Field,
                        NodeKind::Constructor => MemberKind::Constructor,
                        _ => continue,
                    };

                    let child_fqn = self.render_fqn_id(child_id);
                    // Extract member name using unified convention
                    // Members always use '#' separator, so this should always succeed
                    let name = crate::naming::extract_member_name(&child_fqn)
                        .unwrap_or_else(|| self.graph.fqns().resolve_atom(node.name))
                        .to_string();

                    let type_ref = self.extract_type_from_metadata(&node.metadata);

                    members.push(MemberInfo {
                        name,
                        fqn: child_fqn,
                        kind,
                        declaring_type: type_fqn.to_string(),
                        type_ref,
                        parameters: self.extract_parameters(&node.metadata),
                        modifiers: self.extract_modifiers(&node.metadata),
                        generic_signature: None,
                    });
                }
            }
        }
        members
    }
}

/// Iterator over ancestor types (BFS).
struct AncestorIterator<'a> {
    provider: &'a CodeGraphTypeSystem<'a>,
    queue: std::collections::VecDeque<String>,
    visited: std::collections::HashSet<String>,
    max_depth: usize,
    current_depth: usize,
}

impl<'a> AncestorIterator<'a> {
    fn new(provider: &'a CodeGraphTypeSystem<'a>, start: &str, max_depth: usize) -> Self {
        let mut queue = std::collections::VecDeque::new();
        let mut visited = std::collections::HashSet::new();

        // Start with direct parents
        if let Some(super_class) = provider.get_superclass(start) {
            queue.push_back(super_class);
        }
        for iface in provider.get_interfaces(start) {
            queue.push_back(iface);
        }

        visited.insert(start.to_string());

        Self {
            provider,
            queue,
            visited,
            max_depth,
            current_depth: 0,
        }
    }
}

impl<'a> Iterator for AncestorIterator<'a> {
    type Item = String;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current_depth >= self.max_depth {
            return None;
        }

        while let Some(fqn) = self.queue.pop_front() {
            if self.visited.contains(&fqn) {
                continue;
            }

            self.visited.insert(fqn.clone());
            self.current_depth += 1;

            // Add parents of this type
            if let Some(super_class) = self.provider.get_superclass(&fqn) {
                if !self.visited.contains(&super_class) {
                    self.queue.push_back(super_class);
                }
            }
            for iface in self.provider.get_interfaces(&fqn) {
                if !self.visited.contains(&iface) {
                    self.queue.push_back(iface);
                }
            }

            return Some(fqn);
        }

        None
    }
}

/// Iterator over descendant types (BFS).
struct DescendantIterator<'a> {
    provider: &'a CodeGraphTypeSystem<'a>,
    queue: std::collections::VecDeque<String>,
    visited: std::collections::HashSet<String>,
    max_depth: usize,
    current_depth: usize,
}

impl<'a> DescendantIterator<'a> {
    fn new(provider: &'a CodeGraphTypeSystem<'a>, start: &str, max_depth: usize) -> Self {
        let mut queue = std::collections::VecDeque::new();
        let mut visited = std::collections::HashSet::new();

        // Start with direct children
        for sub in provider.get_direct_subtypes(start) {
            queue.push_back(sub);
        }

        visited.insert(start.to_string());

        Self {
            provider,
            queue,
            visited,
            max_depth,
            current_depth: 0,
        }
    }
}

impl<'a> Iterator for DescendantIterator<'a> {
    type Item = String;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current_depth >= self.max_depth {
            return None;
        }

        while let Some(fqn) = self.queue.pop_front() {
            if self.visited.contains(&fqn) {
                continue;
            }

            self.visited.insert(fqn.clone());
            self.current_depth += 1;

            // Add children of this type
            for sub in self.provider.get_direct_subtypes(&fqn) {
                if !self.visited.contains(&sub) {
                    self.queue.push_back(sub);
                }
            }

            return Some(fqn);
        }

        None
    }
}
