use super::*;

impl NaviscopeEngine {
    /// Load index from disk
    pub async fn load(&self) -> Result<bool> {
        let path = self.index_path.clone();
        let lang_caps = self.lang_caps.clone();
        let build_caps = self.build_caps.clone();

        // Load in blocking pool
        let graph_opt =
            tokio::task::spawn_blocking(move || Self::load_from_disk(&path, lang_caps, build_caps))
                .await
                .map_err(|e| NaviscopeError::Internal(e.to_string()))??;

        if let Some(graph) = graph_opt {
            // Atomically update current
            let mut lock = self.current.write().await;
            *lock = Arc::new(graph);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Save current graph to disk
    pub async fn save(&self) -> Result<()> {
        let graph = self.snapshot().await;
        let path = self.index_path.clone();
        let lang_caps = self.lang_caps.clone();
        let build_caps = self.build_caps.clone();

        tokio::task::spawn_blocking(move || {
            Self::save_to_disk(&graph, &path, lang_caps, build_caps)
        })
        .await
        .map_err(|e| NaviscopeError::Internal(e.to_string()))?
    }

    /// Rebuild the index from scratch
    pub async fn rebuild(&self) -> Result<()> {
        let _ = self.scan_global_assets().await;
        let project_root = self.project_root.clone();
        let build_caps = self.build_caps.clone();
        let lang_caps = self.lang_caps.clone();
        let global_routes = self.global_asset_routes();

        let stub_tx = self.stub_tx.clone();
        let (new_graph, stubs) = tokio::task::spawn_blocking(move || {
            Self::build_index(&project_root, build_caps, lang_caps, stub_tx, global_routes)
        })
        .await
        .map_err(|e| NaviscopeError::Internal(e.to_string()))??;

        // Atomically update (write lock held for microseconds)
        {
            let mut lock = self.current.write().await;
            *lock = Arc::new(new_graph);
        }

        // Schedule stubs AFTER graph update using explicit requests
        for req in stubs {
            if let Err(e) = self.stub_tx.send(req.clone()) {
                tracing::warn!("Failed to schedule stub: {}", e);
            }
        }

        // Save to disk
        self.save().await?;

        Ok(())
    }

    /// Update specific files incrementally
    pub async fn update_files(&self, files: Vec<PathBuf>) -> Result<()> {
        let _ = self.scan_global_assets().await;
        let base_graph = self.snapshot().await;
        let build_caps = self.build_caps.clone();
        let lang_caps = self.lang_caps.clone();
        let global_routes = Arc::new(self.global_asset_routes());

        // Prepare existing file metadata for change detection
        let mut existing_metadata = std::collections::HashMap::new();
        for (path, entry) in base_graph.file_index() {
            existing_metadata.insert(
                PathBuf::from(base_graph.symbols().resolve(&path.0)),
                entry.metadata.clone(),
            );
        }

        let current_lock = self.current.clone();
        let stub_tx = self.stub_tx.clone();

        // Processing in blocking pool
        tokio::task::spawn_blocking(move || -> Result<()> {
            let mut manual_ops = Vec::new();
            let mut to_scan = Vec::new();

            for path in files {
                if path.exists() {
                    to_scan.push(path);
                } else {
                    // File was deleted
                    manual_ops.push(GraphOp::RemovePath {
                        path: Arc::from(path.as_path()),
                    });
                }
            }

            // 1. Initial scan to identify file types and changes
            let scan_results = Scanner::scan_files(to_scan, &existing_metadata);
            if scan_results.is_empty() && manual_ops.is_empty() {
                return Ok(());
            }

            // Partition into build and source
            let (build_files, source_files): (Vec<_>, Vec<_>) =
                scan_results.into_iter().partition(|f| f.is_build());

            let resolver = Arc::new(
                IndexResolver::with_caps((*build_caps).clone(), (*lang_caps).clone())
                    .with_stubbing(StubbingManager::new(stub_tx.clone())),
            );

            // 2. Phase 1: Heavy Build Resolution (Global Context)
            let mut project_context_inner = crate::ingest::resolver::ProjectContext::new();
            let mut initial_ops = manual_ops;

            // IMPORTANT: RemovePath MUST come before AddNode for the same paths.
            // Add RemovePath and UpdateFile for build files up front.
            for bf in &build_files {
                initial_ops.push(GraphOp::RemovePath {
                    path: Arc::from(bf.path()),
                });
                initial_ops.push(GraphOp::UpdateFile {
                    metadata: bf.file.clone(),
                });
            }

            // For build files, we still process them up front because they define the structure
            let build_ops =
                resolver.resolve_build_batch(&build_files, &mut project_context_inner)?;
            initial_ops.extend(build_ops);

            let project_context = Arc::new(project_context_inner);
            let routes = global_routes.clone();

            // 3. Phase 2: Source processing via naviscope-ingest streaming runtime.
            let (final_graph, pending_stubs) = crate::runtime::ingest_adapter::run_source_ingest(
                &base_graph,
                initial_ops,
                source_files,
                Arc::clone(&resolver),
                project_context,
                routes,
                lang_caps,
                naviscope_ingest::RuntimeConfig {
                    kernel_channel_capacity: 500,
                    max_in_flight: 256,
                    deferred_poll_limit: 256,
                    idle_sleep_ms: 10,
                },
            )?;

            // 4. Final Swap
            let final_graph = Arc::new(final_graph);
            let rt = tokio::runtime::Handle::current();
            rt.block_on(async {
                let mut lock = current_lock.write().await;
                *lock = final_graph;
            });

            // 5. Schedule stubs
            for req in pending_stubs {
                if let Err(e) = stub_tx.send(req) {
                    tracing::warn!("Failed to schedule stub: {}", e);
                }
            }

            Ok(())
        })
        .await
        .map_err(|e| crate::error::NaviscopeError::Internal(e.to_string()))??;

        // Save at the very end
        self.save().await?;

        Ok(())
    }

    /// Refresh index (detect changes and update)
    pub async fn refresh(&self) -> Result<()> {
        let project_root = self.project_root.clone();

        // Scan for all current files and update incrementally
        let paths = tokio::task::spawn_blocking(move || Scanner::collect_paths(&project_root))
            .await
            .map_err(|e| NaviscopeError::Internal(e.to_string()))?;

        self.update_files(paths).await
    }
}
