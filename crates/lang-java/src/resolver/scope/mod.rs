use super::context::ResolutionContext;
use naviscope_core::ingest::resolver::scope::SemanticScope;

pub trait Scope: for<'a> SemanticScope<ResolutionContext<'a>> {}
impl<T: for<'a> SemanticScope<ResolutionContext<'a>>> Scope for T {}

pub mod builtin;
pub mod import_scope;
pub mod local;
pub mod member;

pub use builtin::BuiltinScope;
pub use import_scope::ImportScope;
pub use local::LocalScope;
pub use member::MemberScope;
