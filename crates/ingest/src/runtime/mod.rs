use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use tokio::sync::{Mutex, Notify, mpsc};

use crate::error::IngestError;
use crate::traits::{CommitSink, DeferredStore, Executor, RuntimeMetrics};
use crate::types::{DependencyReadyEvent, Message, RuntimeConfig};

pub mod flow_control;
pub mod kernel;

pub use flow_control::FlowControlConfig;
pub use kernel::{PipelineBus, TokioPipelineBus};

pub type DynExecutor<P, Op> = Arc<dyn Executor<P, Op> + Send + Sync>;
pub type DynDeferredStore<P> = Arc<dyn DeferredStore<P> + Send + Sync>;
pub type DynCommitSink<Op> = Arc<dyn CommitSink<Op> + Send + Sync>;
pub type DynPipelineBus<P, Op> = Arc<dyn PipelineBus<P, Op> + Send + Sync>;
pub type DynRuntimeMetrics = Arc<dyn RuntimeMetrics + Send + Sync>;

#[derive(Default)]
pub(super) struct EpochTracker {
    inner: std::sync::Mutex<std::collections::HashMap<u64, EpochState>>,
}

struct EpochState {
    submitted: usize,
    committed: usize,
    sealed: bool,
    notify: Arc<Notify>,
}

impl EpochTracker {
    fn record_submit(&self, epoch: u64) -> Result<(), IngestError> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| IngestError::Execution("epoch tracker poisoned".to_string()))?;
        let state = guard.entry(epoch).or_insert_with(|| EpochState {
            submitted: 0,
            committed: 0,
            sealed: false,
            notify: Arc::new(Notify::new()),
        });
        if state.sealed {
            return Err(IngestError::Execution(format!(
                "epoch {epoch} is sealed; no more submissions allowed"
            )));
        }
        state.submitted += 1;
        Ok(())
    }

    pub(super) fn record_internal_submit(&self, epoch: u64) -> Result<(), IngestError> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| IngestError::Execution("epoch tracker poisoned".to_string()))?;
        let state = guard.entry(epoch).or_insert_with(|| EpochState {
            submitted: 0,
            committed: 0,
            sealed: false,
            notify: Arc::new(Notify::new()),
        });
        state.submitted += 1;
        Ok(())
    }

    fn seal(&self, epoch: u64) -> Result<(), IngestError> {
        let notify = {
            let mut guard = self
                .inner
                .lock()
                .map_err(|_| IngestError::Execution("epoch tracker poisoned".to_string()))?;
            let state = guard.entry(epoch).or_insert_with(|| EpochState {
                submitted: 0,
                committed: 0,
                sealed: false,
                notify: Arc::new(Notify::new()),
            });
            state.sealed = true;
            if state.committed >= state.submitted {
                Some(Arc::clone(&state.notify))
            } else {
                None
            }
        };
        if let Some(n) = notify {
            n.notify_waiters();
        }
        Ok(())
    }

    fn rollback_submit(&self, epoch: u64) -> Result<(), IngestError> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| IngestError::Execution("epoch tracker poisoned".to_string()))?;
        let mut should_remove = false;
        if let Some(state) = guard.get_mut(&epoch) {
            if state.submitted > state.committed {
                state.submitted -= 1;
            }
            if !state.sealed && state.submitted == 0 && state.committed == 0 {
                should_remove = true;
            }
        }
        if should_remove {
            guard.remove(&epoch);
        }
        Ok(())
    }

    pub(super) fn mark_committed(&self, epoch: u64) -> Result<(), IngestError> {
        let notify = {
            let mut guard = self
                .inner
                .lock()
                .map_err(|_| IngestError::Execution("epoch tracker poisoned".to_string()))?;
            let state = guard.entry(epoch).or_insert_with(|| EpochState {
                submitted: 0,
                committed: 0,
                sealed: false,
                notify: Arc::new(Notify::new()),
            });
            state.committed += 1;
            if state.sealed && state.committed >= state.submitted {
                Some(Arc::clone(&state.notify))
            } else {
                None
            }
        };
        if let Some(n) = notify {
            n.notify_waiters();
        }
        Ok(())
    }

    async fn wait_epoch(&self, epoch: u64) -> Result<(), IngestError> {
        loop {
            let maybe_notify = {
                let mut guard = self
                    .inner
                    .lock()
                    .map_err(|_| IngestError::Execution("epoch tracker poisoned".to_string()))?;
                let state = guard.entry(epoch).or_insert_with(|| EpochState {
                    submitted: 0,
                    committed: 0,
                    sealed: false,
                    notify: Arc::new(Notify::new()),
                });
                if state.sealed && state.committed >= state.submitted {
                    guard.remove(&epoch);
                    None
                } else {
                    Some(Arc::clone(&state.notify))
                }
            };
            match maybe_notify {
                None => return Ok(()),
                Some(n) => n.notified().await,
            }
        }
    }
}

pub struct RuntimeComponents<P, Op>
where
    P: Clone + Send + Sync + 'static,
    Op: Send + Sync + 'static,
{
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
        executor: DynExecutor<P, Op>,
        deferred_store: DynDeferredStore<P>,
        commit_sink: DynCommitSink<Op>,
        metrics: DynRuntimeMetrics,
    ) -> Self {
        Self {
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
    next_epoch: Arc<AtomicU64>,
    epoch_tracker: Arc<EpochTracker>,
}

impl<P> IntakeHandle<P>
where
    P: Clone + Send + Sync + 'static,
{
    pub fn try_submit(&self, message: Message<P>) -> Result<(), IngestError> {
        let epoch = message.epoch;
        self.epoch_tracker.record_submit(epoch)?;
        self.tx.try_send(message).map_err(|err| {
            let _ = self.epoch_tracker.rollback_submit(epoch);
            match err {
            tokio::sync::mpsc::error::TrySendError::Full(_) => {
                IngestError::Execution("ingest intake queue full".to_string())
            }
            tokio::sync::mpsc::error::TrySendError::Closed(_) => {
                IngestError::Execution("ingest intake handle closed".to_string())
            }
        }})
    }

    pub async fn submit(&self, message: Message<P>) -> Result<(), IngestError> {
        let epoch = message.epoch;
        self.epoch_tracker.record_submit(epoch)?;
        self.tx
            .send(message)
            .await
            .map_err(|_| {
                let _ = self.epoch_tracker.rollback_submit(epoch);
                IngestError::Execution("ingest intake handle closed".to_string())
            })
    }

    pub fn new_epoch(&self) -> u64 {
        self.next_epoch.fetch_add(1, Ordering::Relaxed)
    }

    pub fn seal_epoch(&self, epoch: u64) -> Result<(), IngestError> {
        self.epoch_tracker.seal(epoch)
    }

    pub async fn wait_epoch(&self, epoch: u64) -> Result<(), IngestError> {
        self.epoch_tracker.wait_epoch(epoch).await
    }
}

pub struct IngestRuntime<P, Op>
where
    P: Clone + Send + Sync + 'static,
    Op: Send + Sync + 'static,
{
    pub executor: DynExecutor<P, Op>,
    pub deferred_store: DynDeferredStore<P>,
    pub commit_sink: DynCommitSink<Op>,
    pub bus: DynPipelineBus<P, Op>,
    pub metrics: DynRuntimeMetrics,
    pub flow_control: FlowControlConfig,
    intake_tx: mpsc::Sender<Message<P>>,
    next_epoch: Arc<AtomicU64>,
    epoch_tracker: Arc<EpochTracker>,
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
        let epoch_tracker = Arc::new(EpochTracker::default());

        Self {
            executor: components.executor,
            deferred_store: components.deferred_store,
            commit_sink: components.commit_sink,
            bus: components.bus,
            metrics: components.metrics,
            flow_control,
            intake_tx,
            next_epoch: Arc::new(AtomicU64::new(1)),
            epoch_tracker,
            pipeline_channels: Mutex::new(Some(channels)),
            _marker: std::marker::PhantomData,
        }
    }

    pub fn intake_handle(&self) -> IntakeHandle<P> {
        IntakeHandle {
            tx: self.intake_tx.clone(),
            next_epoch: Arc::clone(&self.next_epoch),
            epoch_tracker: Arc::clone(&self.epoch_tracker),
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
        let executor = Arc::clone(&self.executor);
        let deferred_store = Arc::clone(&self.deferred_store);
        let commit_sink = Arc::clone(&self.commit_sink);
        let metrics = Arc::clone(&self.metrics);
        let epoch_tracker = Arc::clone(&self.epoch_tracker);
        let kernel_task = tokio::spawn(async move {
            kernel::run_pipeline_with_epoch_tracker(
                channels,
                executor,
                deferred_store,
                commit_sink,
                metrics,
                Some(epoch_tracker.clone()),
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
