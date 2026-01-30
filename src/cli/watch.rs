use naviscope::engine::NaviscopeEngine;
use naviscope::project::watcher::Watcher;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;
use tracing::{error, info};

pub fn run(path: PathBuf, debug: bool) -> Result<(), Box<dyn std::error::Error>> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    let engine = NaviscopeEngine::new(path.clone());
    info!("Initializing: Indexing project at: {}...", path.display());
    rt.block_on(engine.rebuild())?;
    info!("Initial indexing complete. Ready to watch for changes.");

    let mut watcher = Watcher::new(&path)?;

    loop {
        // Wait for the first event
        if let Some(event) = watcher.next_event() {
            if !event
                .paths
                .iter()
                .any(|p| naviscope::project::is_relevant_path(p))
            {
                continue;
            }

            // Debounce: wait for more events in the next 500ms
            thread::sleep(Duration::from_millis(500));

            // Drain all pending events
            while watcher.try_next_event().is_some() {}

            info!("Change detected. Re-indexing...");
            match rt.block_on(engine.rebuild()) {
                Ok(_) => {
                    let index = rt.block_on(engine.snapshot());
                    info!(
                        "Indexing complete! Nodes: {}, Edges: {}",
                        index.node_count(),
                        index.edge_count()
                    );

                    if debug {
                        let json_path = PathBuf::from("naviscope_debug.json");
                        index.save_to_json(json_path)?;
                    }
                }
                Err(e) => error!("Error during re-indexing: {}", e),
            }
        }
    }
}
