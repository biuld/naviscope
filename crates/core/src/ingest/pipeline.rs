use crate::error::Result;
use crate::model::GraphOp;
use std::path::PathBuf;

/// Ingest task context, used to share state between batches
pub trait PipelineContext: Send + Sync {}

/// A processing stage of the pipeline
pub trait PipelineStage<C: PipelineContext>: Send + Sync {
    type Output;

    /// Processes a batch of paths
    fn process(&self, context: &C, paths: Vec<PathBuf>) -> Result<Vec<Self::Output>>;
}

/// Ingest pipeline engine
pub struct IngestPipeline {
    batch_size: usize,
}

impl IngestPipeline {
    pub fn new(batch_size: usize) -> Self {
        Self {
            batch_size: if batch_size == 0 { 100 } else { batch_size },
        }
    }

    /// Executes the pipeline: processes paths in chunks and commits the products
    pub fn execute<C, S, F>(
        &self,
        context: &C,
        paths: Vec<PathBuf>,
        stage: &S,
        mut committer: F,
    ) -> Result<()>
    where
        C: PipelineContext,
        S: PipelineStage<C, Output = GraphOp>,
        F: FnMut(Vec<GraphOp>) -> Result<()>,
    {
        for chunk in paths.chunks(self.batch_size) {
            // 1. Process the current batch
            let outputs = stage.process(context, chunk.to_vec())?;

            // 2. Commit the products
            committer(outputs)?;

            // End of batch, local variables are cleaned up
        }
        Ok(())
    }
}
