use naviscope::index::Naviscope;
use naviscope::project::watcher::Watcher;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;
use tracing::{info, error};

pub fn run(path: PathBuf, debug: bool) -> Result<(), Box<dyn std::error::Error>> {
    let mut engine = Naviscope::new(path.clone());
    info!("Initializing: Indexing project at: {}...", path.display());
    engine.build_index()?;
    info!("Initial indexing complete. Ready to watch for changes.");

    let mut watcher = Watcher::new(&path)?;
    
    loop {
        // Wait for the first event
        if let Some(event) = watcher.next_event() {
            if !event.paths.iter().any(|p| naviscope::project::is_relevant_path(p)) {
                continue;
            }

            // Debounce: wait for more events in the next 500ms
            thread::sleep(Duration::from_millis(500));
            
            // Drain all pending events
            while watcher.try_next_event().is_some() {}

            info!("Change detected. Re-indexing...");
            match engine.build_index() {
                Ok(_) => {
                    let index = engine.graph();
                    info!(
                        "Indexing complete! Nodes: {}, Edges: {}",
                        index.topology.node_count(),
                        index.topology.edge_count()
                    );
                    
                    if debug {
                        let json_path = PathBuf::from("naviscope_debug.json");
                        engine.save_to_json(json_path)?;
                    }
                }
                Err(e) => error!("Error during re-indexing: {}", e),
            }
        }
    }
}
