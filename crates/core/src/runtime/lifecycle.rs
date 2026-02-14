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
        {
            let mut lock = self.current.write().await;
            *lock = Arc::new(CodeGraph::empty());
        }

        let project_root = self.project_root.clone();
        let paths = tokio::task::spawn_blocking(move || Scanner::collect_paths(&project_root))
            .await
            .map_err(|e| NaviscopeError::Internal(e.to_string()))?;

        self.update_files(paths).await
    }

    /// Update specific files incrementally
    pub async fn update_files(&self, files: Vec<PathBuf>) -> Result<()> {
        let _ = self.scan_global_assets().await;
        let base_graph = self.snapshot().await;
        let existing_metadata = Self::collect_existing_metadata(&base_graph);
        let (graph_after_build, source_paths, project_context) =
            self.run_build_phase(base_graph, files, existing_metadata).await?;
        self.apply_graph_snapshot(graph_after_build).await;
        self.submit_source_stream(source_paths, project_context).await?;
        self.finalize_update().await?;
        Ok(())
    }

    /// Refresh index (detect changes and update)
    pub async fn refresh(&self) -> Result<()> {
        let project_root = self.project_root.clone();

        let paths = tokio::task::spawn_blocking(move || Scanner::collect_paths(&project_root))
            .await
            .map_err(|e| NaviscopeError::Internal(e.to_string()))?;

        self.update_files(paths).await
    }

    async fn ensure_ingest_adapter(
        &self,
    ) -> Result<Arc<crate::ingest::IngestAdapter>> {
        let runtime = self
            .ingest_adapter
            .get_or_try_init(|| async {
                crate::ingest::IngestAdapter::start(
                    self.current.clone(),
                    self.naming_conventions.clone(),
                    self.build_caps.clone(),
                    self.lang_caps.clone(),
                    self.stub_cache.clone(),
                )
                .await
                .map(Arc::new)
            })
            .await
            .map(Arc::clone)?;

        let drained = match self.pending_stub_requests.lock() {
            Ok(mut pending) => pending.drain(..).collect::<Vec<_>>(),
            Err(_) => Vec::new(),
        };
        for req in drained {
            if let Err(err) = runtime.submit_stub_request(req).await {
                tracing::warn!("Failed to submit deferred stub request: {}", err);
            }
        }

        Ok(runtime)
    }

    fn collect_existing_metadata(
        base_graph: &CodeGraph,
    ) -> std::collections::HashMap<PathBuf, crate::model::source::SourceFile> {
        let mut existing_metadata = std::collections::HashMap::new();
        for (path, entry) in base_graph.file_index() {
            existing_metadata.insert(
                PathBuf::from(base_graph.symbols().resolve(&path.0)),
                entry.metadata.clone(),
            );
        }
        existing_metadata
    }

    async fn run_build_phase(
        &self,
        base_graph: CodeGraph,
        files: Vec<PathBuf>,
        existing_metadata: std::collections::HashMap<PathBuf, crate::model::source::SourceFile>,
    ) -> Result<(CodeGraph, Vec<PathBuf>, naviscope_plugin::ProjectContext)> {
        let build_caps = self.build_caps.clone();
        let lang_caps = self.lang_caps.clone();
        tokio::task::spawn_blocking(move || -> Result<_> {
            let mut manual_ops = Vec::new();
            let mut to_scan = Vec::new();

            for path in files {
                if path.exists() {
                    to_scan.push(path);
                } else {
                    manual_ops.push(GraphOp::RemovePath {
                        path: Arc::from(path.as_path()),
                    });
                }
            }

            let mut build_files = Vec::new();
            let mut source_paths = Vec::new();
            for file in Scanner::scan_files_iter(to_scan, &existing_metadata) {
                if build_caps
                    .iter()
                    .any(|caps| caps.matcher.supports_path(file.path()))
                {
                    build_files.push(file);
                } else {
                    source_paths.push(file.path().to_path_buf());
                }
            }

            if build_files.is_empty() && source_paths.is_empty() && manual_ops.is_empty() {
                return Ok((base_graph, Vec::new(), naviscope_plugin::ProjectContext::new()));
            }

            let compiler =
                crate::indexing::compiler::BatchCompiler::with_caps((*build_caps).clone());

            let mut project_context = naviscope_plugin::ProjectContext::new();
            let mut initial_ops = manual_ops;

            for bf in &build_files {
                initial_ops.push(GraphOp::RemovePath {
                    path: Arc::from(bf.path()),
                });
                initial_ops.push(GraphOp::UpdateFile {
                    metadata: bf.file.clone(),
                });
            }

            let build_ops = compiler.compile_build_batch(&build_files, &mut project_context)?;
            initial_ops.extend(build_ops);

            let mut builder = base_graph.to_builder();
            for caps in lang_caps.iter() {
                if let Some(nc) = caps.presentation.naming_convention() {
                    builder.naming_conventions.insert(caps.language.clone(), nc);
                }
            }
            builder.apply_ops(initial_ops)?;

            Ok((builder.build(), source_paths, project_context))
        })
        .await
        .map_err(|e| NaviscopeError::Internal(e.to_string()))?
    }

    async fn apply_graph_snapshot(&self, graph: CodeGraph) {
        let mut lock = self.current.write().await;
        *lock = Arc::new(graph);
    }

    async fn submit_source_stream(
        &self,
        source_paths: Vec<PathBuf>,
        project_context: naviscope_plugin::ProjectContext,
    ) -> Result<()> {
        const SOURCE_SUBMIT_CHUNK_SIZE: usize = 256;
        if source_paths.is_empty() {
            return Ok(());
        }

        let ingest_adapter = self.ensure_ingest_adapter().await?;
        let routes = self.global_asset_routes();

        for chunk in source_paths.chunks(SOURCE_SUBMIT_CHUNK_SIZE) {
            let chunk_paths = chunk.to_vec();
            let source_files = tokio::task::spawn_blocking(move || {
                let existing = std::collections::HashMap::new();
                Scanner::scan_files_iter(chunk_paths, &existing).collect::<Vec<_>>()
            })
            .await
            .map_err(|e| NaviscopeError::Internal(e.to_string()))?;

            if source_files.is_empty() {
                continue;
            }

            ingest_adapter
                .submit_source_batch(source_files, project_context.clone(), routes.clone())
                .await?;
        }
        Ok(())
    }

    async fn finalize_update(&self) -> Result<()> {
        self.save().await
    }
}
