use std::path::PathBuf;
use tracing::info;

pub async fn run(path: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let engine = naviscope_runtime::build_default_engine(path.clone());

    info!("Initializing: Indexing project at: {}...", path.display());
    engine.rebuild().await?;
    info!("Initial indexing complete.");

    // Start background watcher via trait
    let watch_handle = engine.start_watch().await?;
    info!("File watcher started. Ready for changes.");
    info!("Press Ctrl+C to stop.");

    // Keep the main thread alive
    tokio::signal::ctrl_c().await?;
    watch_handle.stop();
    info!("Watcher stopped.");

    Ok(())
}
