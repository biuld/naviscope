use crate::resolver::scope::SemanticScope;
use super::context::ResolutionContext;

pub trait Scope: for<'a> SemanticScope<ResolutionContext<'a>> {}
impl<T: for<'a> SemanticScope<ResolutionContext<'a>>> Scope for T {}

pub mod local;
pub mod member;
pub mod import_scope;
pub mod builtin;

pub use local::LocalScope;
pub use member::MemberScope;
pub use import_scope::ImportScope;
pub use builtin::BuiltinScope;
