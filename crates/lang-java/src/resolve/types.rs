use crate::JavaPlugin;
use crate::inference::{TypeProvider, TypeResolutionContext};
use naviscope_api::models::TypeRef;

impl JavaPlugin {
    pub(crate) fn resolve_type_ref(
        &self,
        type_ref: &TypeRef,
        provider: &dyn TypeProvider,
        ctx: &TypeResolutionContext,
    ) -> TypeRef {
        match type_ref {
            TypeRef::Raw(name) => {
                // 1. Check if name matches a known FQN suffix in the same file (Inner class priority)
                if let Some(fqn) = ctx
                    .known_fqns
                    .iter()
                    .find(|k| k.ends_with(&format!(".{}", name)) || *k == name)
                {
                    return TypeRef::Id(fqn.clone());
                }

                // 2. Delegate to TypeProvider
                if let Some(fqn) = provider.resolve_type_name(name, ctx) {
                    TypeRef::Id(fqn)
                } else {
                    TypeRef::Raw(name.clone())
                }
            }
            TypeRef::Generic { base, args } => TypeRef::Generic {
                base: Box::new(self.resolve_type_ref(base, provider, ctx)),
                args: args
                    .iter()
                    .map(|a| self.resolve_type_ref(a, provider, ctx))
                    .collect(),
            },
            TypeRef::Array {
                element,
                dimensions,
            } => TypeRef::Array {
                element: Box::new(self.resolve_type_ref(element, provider, ctx)),
                dimensions: *dimensions,
            },
            TypeRef::Wildcard {
                bound,
                is_upper_bound,
            } => TypeRef::Wildcard {
                bound: bound
                    .as_ref()
                    .map(|b| Box::new(self.resolve_type_ref(b, provider, ctx))),
                is_upper_bound: *is_upper_bound,
            },
            _ => type_ref.clone(),
        }
    }
}
