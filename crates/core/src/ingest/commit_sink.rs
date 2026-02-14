use std::collections::HashMap;
use std::sync::Arc;

use naviscope_ingest::{CommitSink, ExecutionResult, IngestError};
use naviscope_plugin::NamingConvention;

use crate::model::{CodeGraph, GraphOp};

pub struct CommitGraphSink {
    pub current: Arc<tokio::sync::RwLock<Arc<CodeGraph>>>,
    pub naming_conventions: Arc<HashMap<String, Arc<dyn NamingConvention>>>,
}

impl CommitSink<GraphOp> for CommitGraphSink {
    fn commit_epoch(
        &self,
        _epoch: u64,
        results: Vec<ExecutionResult<GraphOp>>,
    ) -> Result<usize, IngestError> {
        if results.is_empty() {
            return Ok(0);
        }

        let mut ops = Vec::new();
        for result in results {
            ops.extend(result.operations);
        }

        if !ops.is_empty() {
            let current = Arc::clone(&self.current);
            let naming_conventions = Arc::clone(&self.naming_conventions);
            tokio::runtime::Handle::current().block_on(async move {
                let mut lock = current.write().await;
                let mut builder = lock.as_ref().to_builder();
                for (lang, naming) in naming_conventions.iter() {
                    builder
                        .naming_conventions
                        .insert(crate::model::Language::new(lang.clone()), Arc::clone(naming));
                }
                builder
                    .apply_ops(ops)
                    .map_err(|e| IngestError::Execution(e.to_string()))?;
                *lock = Arc::new(builder.build());
                Ok::<(), IngestError>(())
            })?;
        }

        Ok(1)
    }
}
