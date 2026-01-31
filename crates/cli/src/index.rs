use std::path::PathBuf;
use tracing::info;

pub fn run(path: PathBuf, _debug: bool) -> Result<(), Box<dyn std::error::Error>> {
    let engine = naviscope_runtime::build_default_engine(path.clone());

    info!("Indexing project at: {}...", path.display());

    // Run async build in blocking context
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(engine.rebuild())?;

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    let stats = rt.block_on(engine.get_stats())?;

    info!("Indexing complete!");
    info!("Nodes: {}", stats.node_count);
    info!("Edges: {}", stats.edge_count);

    info!("Sample nodes:");
    let query = naviscope_api::models::GraphQuery::Ls {
        fqn: None,
        kind: vec![],
        modifiers: vec![],
    };
    if let Ok(res) = rt.block_on(engine.query(&query)) {
        for node in res.nodes.iter().take(10) {
            info!(" - {}", node.id);
        }
    }

    Ok(())
}
