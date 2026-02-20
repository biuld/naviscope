use naviscope_api::NaviscopeEngine;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_lsp::Client;
use tower_lsp::lsp_types::MessageType;

pub fn spawn_indexer(
    path: PathBuf,
    client: Client,
    engine_lock: Arc<RwLock<Option<Arc<dyn NaviscopeEngine>>>>,
) {
    tokio::spawn(async move {
        let start = std::time::Instant::now();
        client
            .log_message(
                MessageType::INFO,
                format!("Naviscope indexing started for {:?}", path),
            )
            .await;

        // Retrieve existing handle
        let engine = {
            let lock = engine_lock.read().await;
            match lock.as_ref() {
                Some(h) => h.clone(),
                None => {
                    client
                        .log_message(MessageType::ERROR, "Engine handle not initialized")
                        .await;
                    return;
                }
            }
        };

        // 1. Initial full index rebuild
        if let Err(e) = engine.rebuild().await {
            client
                .log_message(
                    MessageType::ERROR,
                    format!("Initial indexing failed: {}", e),
                )
                .await;
            return;
        }

        let duration = start.elapsed();
        let stats_msg = match engine.get_stats().await {
            Ok(stats) => format!(
                "Initial indexing complete in {:?}: {} nodes, {} edges",
                duration, stats.node_count, stats.edge_count
            ),
            Err(e) => format!(
                "Initial indexing complete in {:?}, but failed to get stats: {}",
                duration, e
            ),
        };
        client.log_message(MessageType::INFO, stats_msg).await;

        // 2. Setup file watcher
        if let Err(e) = engine.start_watch().await {
            client
                .log_message(
                    MessageType::ERROR,
                    format!("Failed to start file watcher: {}", e),
                )
                .await;
        } else {
            client
                .log_message(MessageType::INFO, "File watcher started successfully.")
                .await;
        }
    });
}
