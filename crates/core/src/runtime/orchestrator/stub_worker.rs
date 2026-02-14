use super::*;

impl NaviscopeEngine {
    /// Start the background stubbing worker
    pub(super) fn spawn_stub_worker(
        &self,
        mut rx: tokio::sync::mpsc::UnboundedReceiver<StubRequest>,
        cancel_token: tokio_util::sync::CancellationToken,
        stub_cache: Arc<crate::cache::GlobalStubCache>,
    ) {
        let current = self.current.clone();
        let lang_caps = self.lang_caps.clone();
        let naming_conventions = self.naming_conventions.clone();

        tokio::spawn(async move {
            tracing::info!("Stubbing worker started");
            let mut seen_fqns = std::collections::HashSet::new();

            loop {
                tokio::select! {
                    _ = cancel_token.cancelled() => break,
                    Some(req) = rx.recv() => {
                        // Skip if already seen in this session to avoid redundant work
                        if !seen_fqns.insert(req.fqn.clone()) {
                            continue;
                        }

                        // Check if node already exists and is resolved
                        {
                            let lock = current.read().await;
                            let graph = &**lock;
                            if let Some(idx) = graph.find_node(&req.fqn) {
                                if let Some(node) = graph.get_node(idx) {
                                    if node.status == naviscope_api::models::graph::ResolutionStatus::Resolved {
                                        continue;
                                    }
                                }
                            }
                        }

                        // Resolve
                        let mut ops = Vec::new();

                        for asset_path in req.candidate_paths {
                            // Try to create asset key for cache lookup
                            let asset_key = crate::cache::AssetKey::from_path(&asset_path).ok();

                            // Check cache first
                            if let Some(ref key) = asset_key {
                                if let Some(cached_stub) = stub_cache.lookup(key, &req.fqn) {
                                    tracing::trace!("Cache hit for {}", req.fqn);
                                    ops.push(GraphOp::AddNode {
                                        data: Some(cached_stub),
                                    });
                                    break; // Found it
                                }
                            }

                            // If not in cache, generate stub
                            for caps in lang_caps.iter() {
                                let Some(generator) = caps.asset.stub_generator() else {
                                    continue;
                                };
                                if !generator.can_generate(&asset_path) {
                                    continue;
                                }

                                let entry = AssetEntry::new(asset_path.clone(), AssetSource::Unknown);
                                match generator.generate(&req.fqn, &entry) {
                                    Ok(stub) => {
                                        // Store in cache for future use
                                        if let Some(ref key) = asset_key {
                                            stub_cache.store(key, &stub);
                                            tracing::trace!("Cached stub for {}", req.fqn);
                                        }
                                        ops.push(GraphOp::AddNode { data: Some(stub) });
                                        break;
                                    }
                                    Err(e) => {
                                        tracing::debug!(
                                            "Failed to generate stub for {}: {}",
                                            req.fqn,
                                            e
                                        );
                                    }
                                }
                            }

                            if !ops.is_empty() {
                                break;
                            }
                        }

                        if !ops.is_empty() {
                            let mut lock = current.write().await;
                            let mut builder = (**lock).to_builder();

                            // Load naming conventions
                            let conventions = (*naming_conventions).clone();
                            for (lang, nc) in conventions {
                                builder.naming_conventions.insert(naviscope_api::models::Language::from(lang), nc);
                            }

                            if let Ok(()) = builder.apply_ops(ops) {
                                *lock = Arc::new(builder.build());
                                tracing::trace!("Applied stub for {}", req.fqn);
                            }
                        }
                    }
                }
            }
            tracing::info!("Stubbing worker stopped");
        });
    }
}
