use naviscope_core::engine::NaviscopeEngine;
use std::path::PathBuf;
use tracing::info;

pub fn run(path: Option<PathBuf>) -> Result<(), Box<dyn std::error::Error>> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    if let Some(path) = path {
        let engine = NaviscopeEngine::new(path.clone());
        info!("Clearing index for project at: {}...", path.display());
        rt.block_on(engine.clear_project_index())?;
        info!("Project index cleared.");
    } else {
        info!(
            "Clearing all indices at: {}...",
            NaviscopeEngine::get_base_index_dir().display()
        );
        NaviscopeEngine::clear_all_indices()?;
        info!("All indices cleared.");
    }
    Ok(())
}
