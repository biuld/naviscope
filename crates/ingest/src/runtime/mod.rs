use std::sync::Arc;

use tokio::sync::{Mutex, mpsc};

use crate::error::IngestError;
use crate::traits::{CommitSink, DeferredStore, Executor, RuntimeMetrics, Scheduler};
use crate::types::{DependencyReadyEvent, Message, RuntimeConfig};

pub mod flow_control;
pub mod kernel;

pub use flow_control::FlowControlConfig;
pub use kernel::{PipelineBus, TokioPipelineBus};

pub type DynScheduler<P, Op> = Arc<dyn Scheduler<P, Op> + Send + Sync>;
pub type DynExecutor<P, Op> = Arc<dyn Executor<P, Op> + Send + Sync>;
pub type DynDeferredStore<P> = Arc<dyn DeferredStore<P> + Send + Sync>;
pub type DynCommitSink<Op> = Arc<dyn CommitSink<Op> + Send + Sync>;
pub type DynPipelineBus<P, Op> = Arc<dyn PipelineBus<P, Op> + Send + Sync>;
pub type DynRuntimeMetrics = Arc<dyn RuntimeMetrics + Send + Sync>;

pub struct RuntimeComponents<P, Op>
where
    P: Clone + Send + Sync + 'static,
    Op: Send + Sync + 'static,
{
    pub scheduler: DynScheduler<P, Op>,
    pub executor: DynExecutor<P, Op>,
    pub deferred_store: DynDeferredStore<P>,
    pub commit_sink: DynCommitSink<Op>,
    pub bus: DynPipelineBus<P, Op>,
    pub metrics: DynRuntimeMetrics,
}

impl<P, Op> RuntimeComponents<P, Op>
where
    P: Clone + Send + Sync + 'static,
    Op: Send + Sync + 'static,
{
    pub fn with_tokio_bus(
        scheduler: DynScheduler<P, Op>,
        executor: DynExecutor<P, Op>,
        deferred_store: DynDeferredStore<P>,
        commit_sink: DynCommitSink<Op>,
        metrics: DynRuntimeMetrics,
    ) -> Self {
        Self {
            scheduler,
            executor,
            deferred_store,
            commit_sink,
            bus: Arc::new(TokioPipelineBus),
            metrics,
        }
    }
}

#[derive(Clone)]
pub struct IntakeHandle<P> {
    tx: mpsc::Sender<Message<P>>,
}

impl<P> IntakeHandle<P>
where
    P: Clone + Send + Sync + 'static,
{
    pub async fn submit(&self, message: Message<P>) -> Result<(), IngestError> {
        self.tx
            .send(message)
            .await
            .map_err(|_| IngestError::Execution("ingest intake handle closed".to_string()))
    }
}

pub struct IngestRuntime<P, Op>
where
    P: Clone + Send + Sync + 'static,
    Op: Send + Sync + 'static,
{
    pub scheduler: DynScheduler<P, Op>,
    pub executor: DynExecutor<P, Op>,
    pub deferred_store: DynDeferredStore<P>,
    pub commit_sink: DynCommitSink<Op>,
    pub bus: DynPipelineBus<P, Op>,
    pub metrics: DynRuntimeMetrics,
    pub flow_control: FlowControlConfig,
    intake_tx: mpsc::Sender<Message<P>>,
    pipeline_channels: Mutex<Option<kernel::BusChannels<P>>>,
    _marker: std::marker::PhantomData<Op>,
}

impl<P, Op> IngestRuntime<P, Op>
where
    P: Clone + Send + Sync + 'static,
    Op: Send + Sync + 'static,
{
    pub fn new(config: RuntimeConfig, components: RuntimeComponents<P, Op>) -> Self {
        let flow_control = FlowControlConfig::from(&config);
        let channels = components.bus.open_channels(flow_control.channel_capacity);
        let intake_tx = channels.intake_tx.clone();

        Self {
            scheduler: components.scheduler,
            executor: components.executor,
            deferred_store: components.deferred_store,
            commit_sink: components.commit_sink,
            bus: components.bus,
            metrics: components.metrics,
            flow_control,
            intake_tx,
            pipeline_channels: Mutex::new(Some(channels)),
            _marker: std::marker::PhantomData,
        }
    }

    pub fn intake_handle(&self) -> IntakeHandle<P> {
        IntakeHandle {
            tx: self.intake_tx.clone(),
        }
    }

    pub async fn notify_dependency_ready(
        &self,
        event: DependencyReadyEvent,
    ) -> Result<(), IngestError> {
        let store = Arc::clone(&self.deferred_store);
        tokio::task::spawn_blocking(move || store.notify_ready(event))
            .await
            .map_err(|e| IngestError::Execution(format!("notify task join failure: {e}")))?
    }

    pub async fn run_forever(&self) -> Result<(), IngestError> {
        let channels = self
            .pipeline_channels
            .lock()
            .await
            .take()
            .ok_or_else(|| IngestError::Execution("runtime already started".to_string()))?;

        let flow_control = self.flow_control.clone();
        let scheduler = Arc::clone(&self.scheduler);
        let executor = Arc::clone(&self.executor);
        let deferred_store = Arc::clone(&self.deferred_store);
        let commit_sink = Arc::clone(&self.commit_sink);
        let metrics = Arc::clone(&self.metrics);
        let kernel_task = tokio::spawn(async move {
            kernel::run_pipeline(
                channels,
                scheduler,
                executor,
                deferred_store,
                commit_sink,
                metrics,
                &flow_control,
            )
            .await
        });

        let stats = kernel_task
            .await
            .map_err(|e| IngestError::Execution(format!("kernel task join failure: {e}")))??;

        Err(IngestError::Execution(format!(
            "kernel task exited unexpectedly (runnable={}, deferred_sched={}, deferred_exec={}, deferred_persisted={}, committed={})",
            stats.runnable_messages,
            stats.deferred_from_schedule,
            stats.deferred_from_execute,
            stats.deferred_persisted,
            stats.committed_batches
        )))
    }
}
