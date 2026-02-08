// Re-export standard naming utilities from plugin
pub use naviscope_plugin::naming::{
    MEMBER_SEPARATOR, TYPE_SEPARATOR, build_member_fqn, extract_member_name, extract_type_fqn,
    is_member_fqn, parse_member_fqn,
};

/// Java uses standard dot-separated paths for types and packages,
/// and standard hash-separated paths for members in our graph.
/// Thus we can alias directly to StandardNamingConvention.
pub use naviscope_plugin::StandardNamingConvention as JavaNamingConvention;
