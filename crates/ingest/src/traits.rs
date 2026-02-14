use crate::error::IngestError;
use crate::types::{DependencyReadyEvent, ExecutionResult, Message, PipelineEvent};

pub trait Executor<P, Op>: Send + Sync {
    fn execute(&self, message: Message<P>) -> Result<Vec<PipelineEvent<P, Op>>, IngestError>;
}

pub trait DeferredStore<P>: Send + Sync {
    fn push(&self, message: Message<P>) -> Result<(), IngestError>;
    fn pop_ready(&self, limit: usize) -> Result<Vec<Message<P>>, IngestError>;
    fn notify_ready(&self, event: DependencyReadyEvent) -> Result<(), IngestError>;
}

pub trait CommitSink<Op>: Send + Sync {
    fn commit_epoch(
        &self,
        epoch: u64,
        results: Vec<ExecutionResult<Op>>,
    ) -> Result<usize, IngestError>;
}

pub trait RuntimeMetrics: Send + Sync {
    fn observe_queue_depth(&self, queue: &'static str, depth: usize);
    fn observe_throughput(&self, stage: &'static str, count: usize);
    fn observe_latency_ms(&self, stage: &'static str, p95_ms: u64, p99_ms: u64);
    fn observe_replay_result(&self, ok: bool);
}
