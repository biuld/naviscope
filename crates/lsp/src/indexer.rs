use naviscope_core::engine::handle::EngineHandle;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_lsp::lsp_types::MessageType;
use tower_lsp::Client;

pub fn spawn_indexer(
    path: PathBuf,
    client: Client,
    engine_lock: Arc<RwLock<Option<EngineHandle>>>,
) {
    tokio::spawn(async move {
        let start = std::time::Instant::now();
        client
            .log_message(
                MessageType::INFO,
                format!("Naviscope indexing started for {:?}", path),
            )
            .await;

        // Retrieve existing handle (created by engine_builder in initialize)
        let handle = {
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
        // The handle handles the threading implicitly via spawn_blocking internally if needed,
        // but rebuild() is async so we just await it.
        if let Err(e) = handle.rebuild().await {
            client
                .log_message(
                    MessageType::ERROR,
                    format!("Initial indexing failed: {}", e),
                )
                .await;
            return;
        }

        let duration = start.elapsed();
        let stats = {
            let graph = handle.graph().await;
            format!(
                "Initial indexing complete in {:?}: {} nodes, {} edges",
                duration,
                graph.node_count(),
                graph.edge_count()
            )
        };
        client.log_message(MessageType::INFO, stats).await;

        // 2. Setup file watcher
        if let Err(e) = handle.watch().await {
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
