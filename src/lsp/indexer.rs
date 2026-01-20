use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_lsp::Client;
use tower_lsp::lsp_types::MessageType;
use crate::index::Naviscope;

pub fn spawn_indexer(
    path: PathBuf,
    client: Client,
    naviscope_lock: Arc<RwLock<Option<Naviscope>>>,
) {
    tokio::spawn(async move {
        let start = std::time::Instant::now();
        client.log_message(MessageType::INFO, format!("Naviscope indexing started for {:?}", path)).await;
        let mut navi = Naviscope::new(path.clone());
        
        // 1. Initial full index
        let (res, n) = {
            let mut n = navi;
            tokio::task::spawn_blocking(move || {
                let _ = n.clear_project_index();
                let res = n.build_index();
                (res, n)
            }).await.expect("Indexer task panicked")
        };
        navi = n;

        if let Err(e) = res {
            client.log_message(MessageType::ERROR, format!("Initial indexing failed: {}", e)).await;
        } else {
            let duration = start.elapsed();
            let stats = {
                let n = navi.index().graph.node_count();
                let e = navi.index().graph.edge_count();
                format!("Initial indexing complete in {:?}: {} nodes, {} edges", duration, n, e)
            };
            client.log_message(MessageType::INFO, stats).await;
            
            // Publish the initial index
            {
                let mut lock = naviscope_lock.write().await;
                *lock = Some(navi.clone());
            }
        }

        // 2. Setup file watcher
        use crate::project::watcher::Watcher;
        let watcher = match Watcher::new(&path) {
            Ok(w) => w,
            Err(e) => {
                client.log_message(MessageType::ERROR, format!("Failed to start file watcher: {}", e)).await;
                return;
            }
        };

        // 3. Channel to bridge blocking watcher to async re-indexing
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        
        // Blocking thread for watcher events
        std::thread::spawn(move || {
            while let Some(_) = watcher.next_event() {
                let _ = tx.blocking_send(());
            }
        });

        client.log_message(MessageType::INFO, "File watcher active. Real-time indexing enabled.").await;

        // 4. Watcher loop with debouncing
        while let Some(_) = rx.recv().await {
            // Debounce: wait for 500ms of quiet after the last event
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            // Drain any pending events
            while let Ok(_) = rx.try_recv() {}

            client.log_message(MessageType::INFO, "Change detected on disk, re-indexing...").await;
            let start = std::time::Instant::now();
            
            let (res, n) = {
                let mut n = navi;
                tokio::task::spawn_blocking(move || {
                    let res = n.build_index();
                    (res, n)
                }).await.expect("Indexer task panicked")
            };
            navi = n;

            if let Err(e) = res {
                client.log_message(MessageType::ERROR, format!("Incremental re-indexing failed: {}", e)).await;
            } else {
                let duration = start.elapsed();
                let n = navi.index().graph.node_count();
                let e = navi.index().graph.edge_count();
                client.log_message(MessageType::INFO, format!("Re-indexing complete in {:?}. Total: {} nodes, {} edges", duration, n, e)).await;
                
                // Publish updated index
                let mut lock = naviscope_lock.write().await;
                *lock = Some(navi.clone());
            }
        }
    });
}
