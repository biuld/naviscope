use super::ResolutionContext;
use crate::parser::JavaParser;
use naviscope_api::models::{SymbolIntent, SymbolResolution};
use naviscope_plugin::SemanticScope;

pub struct PackageScope<'a> {
    pub parser: &'a JavaParser,
}

impl<'a, 'b> SemanticScope<ResolutionContext<'a>> for PackageScope<'b> {
    fn resolve(
        &self,
        name: &str,
        context: &ResolutionContext<'a>,
    ) -> Option<Result<SymbolResolution, ()>> {
        // Java Logic: If a name starts with an uppercase letter and we are in a package,
        // it might be a type in the same package.

        // Only trigger for Type intent or Unknown
        if !matches!(context.intent, SymbolIntent::Type | SymbolIntent::Unknown) {
            return None;
        }

        // Heuristic: If it starts with UpperCase, it's likely a class/interface
        if !name.chars().next().map_or(false, |c| c.is_uppercase()) {
            return None;
        }

        if let Some(fqn) = self.parser.resolve_type_name_to_fqn_data(
            name,
            context.package.as_deref(),
            &context.imports,
        ) {
            if fqn.contains('.') && fqn.ends_with(name) {
                return Some(Ok(SymbolResolution::Precise(fqn, SymbolIntent::Type)));
            }
        }

        None
    }

    fn name(&self) -> &'static str {
        "PackageScope"
    }
}
