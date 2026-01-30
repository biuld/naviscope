use naviscope::engine::NaviscopeEngine;
use std::path::PathBuf;
use tracing::info;

pub fn run(path: PathBuf, debug: bool) -> Result<(), Box<dyn std::error::Error>> {
    let engine = NaviscopeEngine::new(path.clone());
    info!("Indexing project at: {}...", path.display());

    // Run async build in blocking context
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(engine.rebuild())?;

    let index = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(engine.snapshot());

    if debug {
        let json_path = PathBuf::from("naviscope_debug.json");
        info!(
            "Debug mode: saving JSON index to: {}...",
            json_path.display()
        );
        index.save_to_json(json_path)?;
    }

    info!("Indexing complete!");
    info!("Nodes: {}", index.node_count());
    info!("Edges: {}", index.edge_count());

    info!("Top 10 nodes:");
    for (fqn, _) in index.fqn_map().iter().take(10) {
        info!(" - {}", fqn);
    }

    Ok(())
}
