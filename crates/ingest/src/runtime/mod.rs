use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{Mutex, mpsc};

use crate::error::IngestError;
use crate::traits::{CommitSink, DeferredStore, Executor, RuntimeMetrics, Scheduler};
use crate::types::{DependencyReadyEvent, Message, RuntimeConfig};

pub mod kernel;

pub use kernel::{KernelConfig, PipelineBus, TokioPipelineBus};

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
    pub kernel_config: KernelConfig,
    deferred_poll_limit: usize,
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
        let kernel_config = KernelConfig::from(&config);
        let channels = components.bus.open_channels(kernel_config.channel_capacity);
        let intake_tx = channels.intake_tx.clone();

        Self {
            scheduler: components.scheduler,
            executor: components.executor,
            deferred_store: components.deferred_store,
            commit_sink: components.commit_sink,
            bus: components.bus,
            metrics: components.metrics,
            kernel_config,
            deferred_poll_limit: config.deferred_poll_limit.max(1),
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
        let replay_tx = self.intake_tx.clone();

        let deferred_store = Arc::clone(&self.deferred_store);
        let deferred_poll_limit = self.deferred_poll_limit;
        let replay_task = {
            let idle_sleep = Duration::from_millis(self.kernel_config.idle_sleep_ms.max(1));
            tokio::spawn(async move {
                loop {
                    let store = Arc::clone(&deferred_store);
                    let ready =
                        tokio::task::spawn_blocking(move || store.pop_ready(deferred_poll_limit))
                            .await
                            .map_err(|e| {
                                IngestError::Execution(format!(
                                    "deferred replay join failure: {e}"
                                ))
                            })??;

                    if ready.is_empty() {
                        tokio::time::sleep(idle_sleep).await;
                        continue;
                    }
                    for msg in ready {
                        replay_tx.send(msg).await.map_err(|_| {
                            IngestError::Execution(
                                "deferred replay failed: intake channel closed".to_string(),
                            )
                        })?;
                    }
                }
                #[allow(unreachable_code)]
                Ok::<(), IngestError>(())
            })
        };

        let kernel_config = self.kernel_config.clone();
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
                &kernel_config,
            )
            .await
        });

        tokio::select! {
            res = replay_task => {
                res.map_err(|e| IngestError::Execution(format!("replay task join failure: {e}")))??;
                Err(IngestError::Execution("replay task exited unexpectedly".to_string()))
            }
            res = kernel_task => {
                let stats = res.map_err(|e| IngestError::Execution(format!("kernel task join failure: {e}")))??;
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
    }
}
