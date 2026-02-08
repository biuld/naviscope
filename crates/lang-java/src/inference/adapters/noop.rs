use crate::inference::core::type_system::{InheritanceProvider, MemberProvider, TypeProvider};
use crate::inference::core::types::{MemberInfo, TypeInfo, TypeResolutionContext};

/// A no-op implementation of JavaTypeSystem.
/// Useful for testing or when global index is unavailable.
pub struct NoOpTypeSystem;

impl TypeProvider for NoOpTypeSystem {
    fn resolve_type_name(&self, _name: &str, _ctx: &TypeResolutionContext) -> Option<String> {
        None
    }

    fn get_type_info(&self, _fqn: &str) -> Option<TypeInfo> {
        None
    }
}

impl InheritanceProvider for NoOpTypeSystem {
    fn get_superclass(&self, _fqn: &str) -> Option<String> {
        None
    }

    fn get_interfaces(&self, _fqn: &str) -> Vec<String> {
        vec![]
    }

    fn walk_ancestors(&self, _fqn: &str) -> Box<dyn Iterator<Item = String> + '_> {
        Box::new(std::iter::empty())
    }

    fn get_direct_subtypes(&self, _fqn: &str) -> Vec<String> {
        vec![]
    }

    fn walk_descendants(&self, _fqn: &str) -> Box<dyn Iterator<Item = String> + '_> {
        Box::new(std::iter::empty())
    }
}

impl MemberProvider for NoOpTypeSystem {
    fn get_members(&self, _type_fqn: &str, _member_name: &str) -> Vec<MemberInfo> {
        vec![]
    }

    fn get_all_members(&self, _type_fqn: &str) -> Vec<MemberInfo> {
        vec![]
    }
}
