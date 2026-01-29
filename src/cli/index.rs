use naviscope::index::Naviscope;
use std::path::PathBuf;
use tracing::info;

pub fn run(path: PathBuf, debug: bool) -> Result<(), Box<dyn std::error::Error>> {
    let mut engine = Naviscope::new(path.clone());
    info!("Indexing project at: {}...", path.display());
    engine.build_index()?;

    if debug {
        let json_path = PathBuf::from("naviscope_debug.json");
        info!(
            "Debug mode: saving JSON index to: {}...",
            json_path.display()
        );
        engine.save_to_json(json_path)?;
    }

    let index = engine.graph();
    info!("Indexing complete!");
    info!("Nodes: {}", index.topology.node_count());
    info!("Edges: {}", index.topology.edge_count());

    info!("Top 10 nodes:");
    for (fqn, _) in index.fqn_map.iter().take(10) {
        info!(" - {}", fqn);
    }

    Ok(())
}
