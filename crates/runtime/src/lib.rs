use naviscope_api::NaviscopeEngine;
use naviscope_api::lifecycle::EngineResult;
use std::path::PathBuf;
use std::sync::Arc;

/// Bootstraps a full-featured Naviscope engine with all available plugins.
///
/// This function acts as the central factory for the Naviscope runtime,
/// assembling the core engine with language-specific plugins like Java and Gradle.
pub fn build_default_engine(path: PathBuf) -> Arc<dyn NaviscopeEngine> {
    let mut builder = naviscope_core::runtime::orchestrator::NaviscopeEngine::builder(path);

    // Register Build Tool Plugins
    builder = builder.with_build_tool(Arc::new(naviscope_gradle::GradlePlugin::new()));

    // Register Language Plugins
    builder = match naviscope_java::JavaPlugin::new() {
        Ok(plugin) => builder.with_language(Arc::new(plugin)),
        Err(e) => {
            tracing::error!("Failed to load Java plugin: {}", e);
            builder
        }
    };

    let engine = builder.build();

    // Wrap in the standard EngineHandle which implements all API traits
    Arc::new(naviscope_core::facade::EngineHandle::from_engine(Arc::new(
        engine,
    )))
}

/// Initializes the logging system for a specific component.
/// This delegates to the core logging module.
pub fn init_logging(component: &str, to_stderr: bool) -> Option<impl Drop> {
    Some(naviscope_core::logging::init_logging(component, to_stderr))
}

/// Utility to clear all indices stored on the local system.
pub fn clear_all_indices() -> EngineResult<()> {
    naviscope_core::runtime::orchestrator::NaviscopeEngine::clear_all_indices().map_err(
        |e: naviscope_core::error::NaviscopeError| {
            naviscope_api::lifecycle::EngineError::Internal(e.to_string())
        },
    )
}

/// Get the global stub cache manager.
pub fn get_cache_manager() -> std::sync::Arc<dyn naviscope_api::StubCacheManager> {
    std::sync::Arc::new(naviscope_core::cache::GlobalStubCache::at_default_location())
}
