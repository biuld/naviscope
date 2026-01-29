use crate::index::Naviscope;
use crate::project::is_relevant_path;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_lsp::Client;
use tower_lsp::lsp_types::MessageType;

pub fn spawn_indexer(path: PathBuf, client: Client, engine_lock: Arc<RwLock<Option<Naviscope>>>) {
    tokio::spawn(async move {
        let start = std::time::Instant::now();
        client
            .log_message(
                MessageType::INFO,
                format!("Naviscope indexing started for {:?}", path),
            )
            .await;
        let mut navi = Naviscope::new(path.clone());

        // 1. Initial full index
        let (res, n) = {
            let mut n = navi;
            tokio::task::spawn_blocking(move || {
                let res = n.build_index();
                (res, n)
            })
            .await
            .expect("Indexer task panicked")
        };
        navi = n;

        if let Err(e) = res {
            client
                .log_message(
                    MessageType::ERROR,
                    format!("Initial indexing failed: {}", e),
                )
                .await;
        } else {
            let duration = start.elapsed();
            let stats = {
                let n = navi.graph().topology.node_count();
                let e = navi.graph().topology.edge_count();
                format!(
                    "Initial indexing complete in {:?}: {} nodes, {} edges",
                    duration, n, e
                )
            };
            client.log_message(MessageType::INFO, stats).await;

            // Publish the initial index
            {
                let mut lock = engine_lock.write().await;
                *lock = Some(navi.clone());
            }
        }

        // 2. Setup file watcher
        use crate::project::watcher::Watcher;
        let mut watcher = match Watcher::new(&path) {
            Ok(w) => w,
            Err(e) => {
                client
                    .log_message(
                        MessageType::ERROR,
                        format!("Failed to start file watcher: {}", e),
                    )
                    .await;
                return;
            }
        };

        client
            .log_message(
                MessageType::INFO,
                "File watcher active. Real-time indexing enabled.",
            )
            .await;

        // 3. Watcher loop with debouncing
        while let Some(res) = watcher.rx.recv().await {
            let event = match res {
                Ok(e) => e,
                Err(_) => continue,
            };

            // Filter relevant paths
            if !event.paths.iter().any(|p| is_relevant_path(p)) {
                continue;
            }

            // Debounce: wait for 500ms of quiet after the last event
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            // Drain any pending events
            while let Ok(_) = watcher.rx.try_recv() {}

            client
                .log_message(MessageType::INFO, "Change detected on disk, re-indexing...")
                .await;
            let start = std::time::Instant::now();

            let (res, n) = {
                let mut n = navi;
                tokio::task::spawn_blocking(move || {
                    let res = n.build_index();
                    (res, n)
                })
                .await
                .expect("Indexer task panicked")
            };
            navi = n;

            if let Err(e) = res {
                client
                    .log_message(
                        MessageType::ERROR,
                        format!("Incremental re-indexing failed: {}", e),
                    )
                    .await;
            } else {
                let duration = start.elapsed();
                let n = navi.graph().topology.node_count();
                let e = navi.graph().topology.edge_count();
                client
                    .log_message(
                        MessageType::INFO,
                        format!(
                            "Re-indexing complete in {:?}. Total: {} nodes, {} edges",
                            duration, n, e
                        ),
                    )
                    .await;

                // Publish updated index
                let mut lock = engine_lock.write().await;
                *lock = Some(navi.clone());
            }
        }
    });
}
