use std::path::PathBuf;
use tracing::info;

pub async fn run(path: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let engine = naviscope_runtime::build_default_engine(path.clone());

    info!("Indexing project at: {}...", path.display());

    // Run async build
    engine.rebuild().await?;

    let stats = engine.get_stats().await?;

    info!("Indexing complete!");
    info!("Nodes: {}", stats.node_count);
    info!("Edges: {}", stats.edge_count);

    info!("Sample nodes:");
    let query = naviscope_api::models::GraphQuery::Ls {
        fqn: None,
        kind: vec![],
        sources: vec![],
        modifiers: vec![],
    };
    if let Ok(res) = engine.query(&query).await {
        for node in res.nodes.iter().take(10) {
            info!(" - {}", node.id);
        }
    }

    Ok(())
}
