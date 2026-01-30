use crate::engine::engine::NaviscopeEngine;
use crate::engine::handle::EngineHandle;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_lsp::Client;
use tower_lsp::lsp_types::MessageType;

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

        // Create the new engine
        let engine = Arc::new(NaviscopeEngine::new(path.clone()));
        let handle = EngineHandle::from_engine(engine);

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

        // Publish the handle
        {
            let mut lock = engine_lock.write().await;
            *lock = Some(handle.clone());
        }

        // 2. Setup file watcher
        // TODO: In Phase 2/3, implement real file watching using handle.watch()
        // For now, we utilize the handle's watch stub or implement a temporary watcher here if needed.
        // Since handle.watch() is a TODO, we can temporarily disable auto-reindexing or
        // keep the old manual watcher logic if critical.
        // Given constraint of Phase 1->2 migration, let's keep it simple first.

        client
            .log_message(
                MessageType::INFO,
                "File watcher placeholder (implementation pending in engine).",
            )
            .await;

        // Note: The previous manual watcher logic is removed in favor of moving
        // watching logic into the EngineHandle in the future.
    });
}
