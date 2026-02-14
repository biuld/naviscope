use super::*;

impl NaviscopeEngine {
    /// Watch for filesystem changes and update incrementally.
    /// The watcher task exits when `cancel_token` is cancelled.
    pub async fn start_watch_with_token(
        self: Arc<Self>,
        cancel_token: tokio_util::sync::CancellationToken,
    ) -> Result<()> {
        use crate::runtime::watcher::Watcher;
        use std::collections::HashSet;
        use std::time::Duration;

        let root = self.project_root.clone();
        let mut watcher =
            Watcher::new(&root).map_err(|e| NaviscopeError::Internal(e.to_string()))?;

        let engine_weak = Arc::downgrade(&self);

        tokio::spawn(async move {
            tracing::info!("Started watching {}", root.display());
            let mut pending_events: Vec<notify::Event> = Vec::new();
            let debounce_interval = Duration::from_millis(500);

            loop {
                tokio::select! {
                    _ = cancel_token.cancelled() => {
                        break;
                    }
                    event = watcher.next_event_async() => {
                        match event {
                            Some(e) => pending_events.push(e),
                            None => break,
                        }
                    }
                    _ = tokio::time::sleep(debounce_interval), if !pending_events.is_empty() => {
                        let mut paths = HashSet::new();
                        for event in &pending_events {
                            for path in &event.paths {
                                if crate::ingest::is_relevant_path(path) {
                                    paths.insert(path.clone());
                                }
                            }
                        }
                        pending_events.clear();

                        if !paths.is_empty() {
                            if let Some(engine) = engine_weak.upgrade() {
                                let path_vec: Vec<_> = paths.into_iter().collect();
                                tracing::info!("Detected changes in {} files. Updating...", path_vec.len());
                                if let Err(err) = engine.update_files(path_vec).await {
                                    tracing::error!("Failed to update files: {}", err);
                                }
                            } else {
                                break;
                            }
                        }
                    }
                }
            }
            tracing::info!("File watcher task ended for {}", root.display());
        });

        Ok(())
    }

    /// Backward-compatible helper that uses the engine-wide cancellation token.
    pub async fn watch(self: Arc<Self>) -> Result<()> {
        let cancel_token = self.cancel_token.clone();
        self.start_watch_with_token(cancel_token).await
    }
}
