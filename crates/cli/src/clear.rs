use std::path::PathBuf;
use tracing::info;

pub fn run(path: Option<PathBuf>) -> Result<(), Box<dyn std::error::Error>> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    if let Some(path) = path {
        let engine = naviscope_runtime::build_default_engine(path.clone());
        info!("Clearing index for project at: {}...", path.display());
        rt.block_on(engine.clear_index())?;
        info!("Project index cleared.");
    } else {
        // For clearing ALL indices, we use the runtime utility.
        info!("Clearing all indices...");
        naviscope_runtime::clear_all_indices()?;
        info!("All indices cleared.");
    }
    Ok(())
}
