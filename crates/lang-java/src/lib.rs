pub mod cap;
pub mod discoverer;
pub mod inference;
pub mod jdk;
pub mod lsp;
pub mod model;
pub mod naming;
pub mod parser;
pub mod queries;
pub mod resolve;

pub use cap::java_caps;
pub use discoverer::JdkDiscoverer;

use std::sync::Arc;

#[derive(Clone)]
pub struct JavaPlugin {
    pub(crate) parser: Arc<parser::JavaParser>,
    pub(crate) type_system: Arc<lsp::type_system::JavaTypeSystem>,
}

impl JavaPlugin {
    pub fn new() -> std::result::Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        naviscope_plugin::register_metadata_deserializer(
            "java",
            crate::model::JavaIndexMetadata::deserialize_for_cache,
        );

        let parser = Arc::new(parser::JavaParser::new()?);
        let type_system = Arc::new(lsp::type_system::JavaTypeSystem::new());
        Ok(Self {
            parser,
            type_system,
        })
    }
}
