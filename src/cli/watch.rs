use naviscope::index::Naviscope;
use naviscope::project::watcher::Watcher;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

pub fn run(path: PathBuf, debug: bool) -> Result<(), Box<dyn std::error::Error>> {
    let mut naviscope = Naviscope::new(path.clone());
    println!("Initializing: Indexing project at: {}...", path.display());
    naviscope.build_index()?;
    println!("Initial indexing complete. Ready to watch for changes.");

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

            println!("Change detected. Re-indexing...");
            match naviscope.build_index() {
                Ok(_) => {
                    let index = naviscope.index();
                    println!(
                        "Indexing complete! Nodes: {}, Edges: {}",
                        index.graph.node_count(),
                        index.graph.edge_count()
                    );
                    
                    if debug {
                        let json_path = PathBuf::from("naviscope_debug.json");
                        naviscope.save_to_json(json_path)?;
                    }
                }
                Err(e) => eprintln!("Error during re-indexing: {}", e),
            }
        }
    }
}
