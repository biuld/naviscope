use crate::error::{NaviscopeError, Result};
use tree_sitter::{Language, Query};

/// Loads a Tree-sitter query from an SCM string.
pub fn load_query(language: &Language, scm: &str) -> Result<Query> {
    Query::new(language, scm)
        .map_err(|e| NaviscopeError::Parsing(format!("Invalid query: {:?}", e)))
}

/// Gets the index of a capture name in a query.
pub fn get_capture_index(query: &Query, name: &str) -> Result<u32> {
    query
        .capture_index_for_name(name)
        .ok_or_else(|| NaviscopeError::Parsing(format!("Capture name '{}' not found in SCM", name)))
}

/// Macro to define a struct for capture indices and a `new` method to initialize it from a query.
#[macro_export]
macro_rules! decl_indices {
    ($name:ident, { $($field:ident => $capture:expr),+ $(,)? }) => {
        pub struct $name {
            $(pub $field: u32,)+
        }

        impl $name {
            pub fn new(query: &tree_sitter::Query) -> $crate::error::Result<Self> {
                Ok(Self {
                    $($field: $crate::parser::utils::get_capture_index(query, $capture)?,)+
                })
            }
        }
    };
}
