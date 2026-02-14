use super::*;

impl NaviscopeEngine {
    /// Clear the index for the current project
    pub async fn clear_project_index(&self) -> Result<()> {
        let path = self.index_path.clone();
        if path.exists() {
            tokio::fs::remove_file(path).await?;
        }

        // Reset current graph
        let mut lock = self.current.write().await;
        *lock = Arc::new(CodeGraph::empty());

        Ok(())
    }

    /// Clear all indices
    pub fn clear_all_indices() -> Result<()> {
        let base_dir = Self::get_base_index_dir();
        if base_dir.exists() {
            std::fs::remove_dir_all(&base_dir)?;
        }
        Ok(())
    }

    /// Gets the base directory for storing indices, supporting NAVISCOPE_INDEX_DIR env var.
    pub fn get_base_index_dir() -> PathBuf {
        if let Ok(env_dir) = std::env::var("NAVISCOPE_INDEX_DIR") {
            return PathBuf::from(env_dir);
        }

        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        Path::new(&home).join(super::super::DEFAULT_INDEX_DIR)
    }

    // ---- Helper methods ----

    pub(super) fn load_from_disk(
        path: &Path,
        lang_caps: Arc<Vec<LanguageCaps>>,
        build_caps: Arc<Vec<BuildCaps>>,
    ) -> Result<Option<CodeGraph>> {
        if !path.exists() {
            return Ok(None);
        }

        let bytes = std::fs::read(path)?;

        let get_codec = |lang: &str| -> Option<Arc<dyn crate::bridge::NodeMetadataCodec>> {
            for caps in lang_caps.iter() {
                if caps.language.as_str() == lang {
                    return caps.metadata_codec.metadata_codec();
                }
            }
            for caps in build_caps.iter() {
                if caps.build_tool.as_str() == lang {
                    return caps.metadata_codec.metadata_codec();
                }
            }
            None
        };

        match CodeGraph::deserialize(&bytes, get_codec) {
            Ok(graph) => {
                if graph.version() != crate::model::graph::CURRENT_VERSION {
                    tracing::warn!(
                        "Index version mismatch at {} (found {}, expected {}). Will rebuild.",
                        path.display(),
                        graph.version(),
                        crate::model::graph::CURRENT_VERSION
                    );
                    let _ = std::fs::remove_file(path);
                    return Ok(None);
                }
                tracing::info!("Loaded index from {}", path.display());
                Ok(Some(graph))
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to parse index at {}: {:?}. Will rebuild.",
                    path.display(),
                    e
                );
                let _ = std::fs::remove_file(path);
                Ok(None)
            }
        }
    }

    pub(super) fn save_to_disk(
        graph: &CodeGraph,
        path: &Path,
        lang_caps: Arc<Vec<LanguageCaps>>,
        build_caps: Arc<Vec<BuildCaps>>,
    ) -> Result<()> {
        // Ensure directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let get_codec = |lang: &str| -> Option<Arc<dyn crate::bridge::NodeMetadataCodec>> {
            for caps in lang_caps.iter() {
                if caps.language.as_str() == lang {
                    return caps.metadata_codec.metadata_codec();
                }
            }
            for caps in build_caps.iter() {
                if caps.build_tool.as_str() == lang {
                    return caps.metadata_codec.metadata_codec();
                }
            }
            None
        };

        // Serialize the graph
        let bytes = graph.serialize(get_codec)?;

        // Write to file atomically (write to temp, then rename)
        let temp_path = path.with_extension("tmp");
        std::fs::write(&temp_path, bytes)?;
        std::fs::rename(temp_path, path)?;

        tracing::info!("Saved index to {}", path.display());

        Ok(())
    }

    pub(super) fn build_index(
        project_root: &Path,
        build_caps: Arc<Vec<BuildCaps>>,
        lang_caps: Arc<Vec<LanguageCaps>>,
        stub_tx: tokio::sync::mpsc::UnboundedSender<StubRequest>,
        global_routes: HashMap<String, Vec<PathBuf>>,
    ) -> Result<(CodeGraph, Vec<StubRequest>)> {
        // Scan and parse
        let parse_results =
            Scanner::scan_and_parse(project_root, &std::collections::HashMap::new());

        // Resolve
        let resolver = IndexResolver::with_caps((*build_caps).clone(), (*lang_caps).clone())
            .with_stubbing(StubbingManager::new(stub_tx));

        // resolve() now returns both ops and the filled ProjectContext
        let (ops, _project_context) = resolver.resolve(parse_results)?;

        // Build graph
        let mut builder = CodeGraphBuilder::new();

        // Register naming conventions
        for caps in lang_caps.iter() {
            if let Some(nc) = caps.presentation.naming_convention() {
                builder.naming_conventions.insert(caps.language.clone(), nc);
            }
        }

        builder.apply_ops(ops.clone())?;

        let stubs = resolver.resolve_stubs(&ops, &global_routes);

        Ok((builder.build(), stubs))
    }

    pub fn get_stub_cache(&self) -> Arc<crate::cache::GlobalStubCache> {
        self.stub_cache.clone()
    }
}
