use std::path::PathBuf;
use tracing::info;

pub fn run(path: PathBuf, _debug: bool) -> Result<(), Box<dyn std::error::Error>> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    let engine = naviscope_runtime::build_default_engine(path.clone());

    info!("Initializing: Indexing project at: {}...", path.display());
    rt.block_on(engine.rebuild())?;
    info!("Initial indexing complete.");

    // Start background watcher via trait
    rt.block_on(engine.watch())?;
    info!("File watcher started. Ready for changes.");
    info!("Press Ctrl+C to stop.");

    // Keep the main thread alive
    rt.block_on(tokio::signal::ctrl_c())?;
    info!("Watcher stopped.");

    Ok(())
}
