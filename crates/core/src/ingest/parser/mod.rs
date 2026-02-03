use crate::error::Result;
use std::path::Path;

pub mod output;
pub mod utils;

pub use naviscope_api::SymbolResolution;
pub use naviscope_api::models::symbol::NodeId;
pub use naviscope_plugin::{GlobalParseResult, IndexNode, IndexRelation, LspParser, ParseOutput};

/// Trait for parsers that provide data for the global code knowledge graph.
pub trait IndexParser: Send + Sync {
    fn parse_file(&self, source_code: &str, file_path: Option<&Path>) -> Result<GlobalParseResult>;
}
