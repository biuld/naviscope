use naviscope_api::NaviscopeEngine;
use std::path::PathBuf;
use std::sync::Arc;

/// Bootstraps a full-featured Naviscope engine with all available plugins.
///
/// This function acts as the central factory for the Naviscope runtime,
/// assembling the core engine with language-specific plugins like Java and Gradle.
pub fn build_default_engine(path: PathBuf) -> Arc<dyn NaviscopeEngine> {
    let mut engine = naviscope_core::engine::NaviscopeEngine::new(path);

    // Register Build Tool Plugins
    engine.register_build_tool(Arc::new(naviscope_gradle::GradlePlugin::new()));

    // Register Language Plugins
    match naviscope_java::JavaPlugin::new() {
        Ok(plugin) => engine.register_language(Arc::new(plugin)),
        Err(e) => tracing::error!("Failed to load Java plugin: {}", e),
    }

    // Wrap in the standard EngineHandle which implements all API traits
    Arc::new(naviscope_core::engine::handle::EngineHandle::from_engine(
        Arc::new(engine),
    ))
}

/// Initializes the logging system for a specific component.
/// This delegates to the core logging module.
pub fn init_logging(component: &str) -> Option<impl Drop> {
    Some(naviscope_core::logging::init_logging(component))
}

/// Utility to clear all indices stored on the local system.
pub fn clear_all_indices() -> naviscope_api::lifecycle::Result<()> {
    naviscope_core::engine::NaviscopeEngine::clear_all_indices()
        .map_err(|e| naviscope_api::lifecycle::EngineError::Internal(e.to_string()))
}
