use naviscope::index::Naviscope;
use std::path::PathBuf;
use tracing::info;

pub fn run(path: Option<PathBuf>) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(path) = path {
        let engine = Naviscope::new(path.clone());
        info!("Clearing index for project at: {}...", path.display());
        engine.clear_project_index()?;
        info!("Project index cleared.");
    } else {
        info!(
            "Clearing all indices at: {}...",
            Naviscope::get_base_index_dir().display()
        );
        Naviscope::clear_all_indices()?;
        info!("All indices cleared.");
    }
    Ok(())
}
