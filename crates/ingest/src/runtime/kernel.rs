use std::collections::BTreeMap;
use std::sync::Arc;

use tokio::sync::mpsc;
use tokio::task::JoinSet;
use tracing::warn;

use crate::error::IngestError;
use crate::runtime::flow_control::{FlowControlConfig, FlowController};
use crate::runtime::{DynCommitSink, DynDeferredStore, DynExecutor, DynRuntimeMetrics};
use crate::types::{DependencyRef, ExecutionResult, Message, PipelineEvent};

pub struct BusChannels<P>
where
    P: Clone + Send + Sync + 'static,
{
    pub intake_tx: mpsc::Sender<Message<P>>,
    pub intake_rx: mpsc::Receiver<Message<P>>,
    pub deferred_tx: mpsc::Sender<Message<P>>,
    pub deferred_rx: mpsc::Receiver<Message<P>>,
}

pub trait PipelineBus<P, Op>: Send + Sync
where
    P: Clone + Send + Sync + 'static,
    Op: Send + Sync + 'static,
{
    fn open_channels(&self, capacity: usize) -> BusChannels<P>;
}

#[derive(Default)]
pub struct TokioPipelineBus;

impl<P, Op> PipelineBus<P, Op> for TokioPipelineBus
where
    P: Clone + Send + Sync + 'static,
    Op: Send + Sync + 'static,
{
    fn open_channels(&self, capacity: usize) -> BusChannels<P> {
        let cap = capacity.max(1);
        let (intake_tx, intake_rx) = mpsc::channel::<Message<P>>(cap);
        let (deferred_tx, deferred_rx) = mpsc::channel::<Message<P>>(cap);

        BusChannels {
            intake_tx,
            intake_rx,
            deferred_tx,
            deferred_rx,
        }
    }
}

#[derive(Debug, Default)]
pub struct KernelRunStats {
    pub runnable_messages: usize,
    pub deferred_from_schedule: usize,
    pub deferred_from_execute: usize,
    pub deferred_persisted: usize,
    pub committed_batches: usize,
}

#[derive(Default)]
struct MessageRunStats {
    runnable_messages: usize,
    deferred_from_schedule: usize,
    deferred_from_execute: usize,
    committed_batches: usize,
}

impl KernelRunStats {
    fn merge_message_stats(&mut self, msg: MessageRunStats) {
        self.runnable_messages += msg.runnable_messages;
        self.deferred_from_schedule += msg.deferred_from_schedule;
        self.deferred_from_execute += msg.deferred_from_execute;
        self.committed_batches += msg.committed_batches;
    }
}

pub async fn run_pipeline<P, Op>(
    channels: BusChannels<P>,
    executor: DynExecutor<P, Op>,
    deferred_store: DynDeferredStore<P>,
    commit_sink: DynCommitSink<Op>,
    metrics: DynRuntimeMetrics,
    config: &FlowControlConfig,
) -> Result<KernelRunStats, IngestError>
where
    P: Clone + Send + Sync + 'static,
    Op: Send + Sync + 'static,
{
    run_pipeline_with_epoch_tracker(
        channels,
        executor,
        deferred_store,
        commit_sink,
        metrics,
        None,
        config,
    )
    .await
}

pub(crate) async fn run_pipeline_with_epoch_tracker<P, Op>(
    channels: BusChannels<P>,
    executor: DynExecutor<P, Op>,
    deferred_store: DynDeferredStore<P>,
    commit_sink: DynCommitSink<Op>,
    metrics: DynRuntimeMetrics,
    epoch_tracker: Option<Arc<super::EpochTracker>>,
    config: &FlowControlConfig,
) -> Result<KernelRunStats, IngestError>
where
    P: Clone + Send + Sync + 'static,
    Op: Send + Sync + 'static,
{
    let BusChannels {
        intake_tx,
        mut intake_rx,
        deferred_tx,
        mut deferred_rx,
    } = channels;
    drop(intake_tx);

    let flow = FlowController::new(config);
    let mut replay_tick = tokio::time::interval(flow.idle_sleep());

    let mut stats = KernelRunStats::default();
    let mut workers = JoinSet::new();
    let mut intake_closed = false;

    loop {
        // Central event loop:
        // - waits on worker completions, deferred persistence, deferred replay ticks, and intake;
        // - picks exactly one ready branch per iteration, then loops again;
        // - `biased;` gives priority in source order to reduce completed-worker lag.
        tokio::select! {
            biased;

            joined = workers.join_next(), if !workers.is_empty() => {
                match joined {
                    Some(joined) => {
                        let msg_stats = joined
                            .map_err(|e| IngestError::Execution(format!("worker join failure: {e}")))??;
                        stats.merge_message_stats(msg_stats);
                    }
                    None => {}
                }
            }

            maybe_msg = deferred_rx.recv() => {
                match maybe_msg {
                    Some(msg) => {
                        persist_deferred(Arc::clone(&deferred_store), msg).await?;
                        stats.deferred_persisted += 1;
                    }
                    None => {
                        if intake_closed && workers.is_empty() {
                            break;
                        }
                    }
                }
            }

            _ = replay_tick.tick(), if !intake_closed => {
                let ready = pop_ready_messages(
                    Arc::clone(&deferred_store),
                    flow.deferred_poll_limit(),
                ).await?;
                if !ready.is_empty() {
                    metrics.observe_replay_result(true);
                }

                for msg in ready {
                    let permit = flow.acquire_in_flight().await?;
                    let executor_cloned = Arc::clone(&executor);
                    let commit_sink_cloned = Arc::clone(&commit_sink);
                    let deferred_store_cloned = Arc::clone(&deferred_store);
                    let deferred_tx_cloned = deferred_tx.clone();
                    let metrics_cloned = Arc::clone(&metrics);
                    let epoch_tracker_cloned = epoch_tracker.clone();
                    workers.spawn(async move {
                        let _permit = permit;
                        process_message(
                            msg,
                            executor_cloned,
                            commit_sink_cloned,
                            deferred_store_cloned,
                            deferred_tx_cloned,
                            metrics_cloned,
                            epoch_tracker_cloned,
                        )
                        .await
                    });
                }
            }

            maybe_msg = intake_rx.recv(), if !intake_closed => {
                match maybe_msg {
                    Some(msg) => {
                        let permit = flow.acquire_in_flight().await?;
                        let executor_cloned = Arc::clone(&executor);
                        let commit_sink_cloned = Arc::clone(&commit_sink);
                        let deferred_store_cloned = Arc::clone(&deferred_store);
                        let deferred_tx_cloned = deferred_tx.clone();
                        let metrics_cloned = Arc::clone(&metrics);
                        let epoch_tracker_cloned = epoch_tracker.clone();
                        workers.spawn(async move {
                            let _permit = permit;
                            process_message(
                                msg,
                                executor_cloned,
                                commit_sink_cloned,
                                deferred_store_cloned,
                                deferred_tx_cloned,
                                metrics_cloned,
                                epoch_tracker_cloned,
                            )
                            .await
                        });
                    }
                    None => intake_closed = true,
                }
            }
        }

        if intake_closed && workers.is_empty() {
            while let Ok(msg) = deferred_rx.try_recv() {
                persist_deferred(Arc::clone(&deferred_store), msg).await?;
                stats.deferred_persisted += 1;
            }

            drop(deferred_tx);
            while let Some(msg) = deferred_rx.recv().await {
                persist_deferred(Arc::clone(&deferred_store), msg).await?;
                stats.deferred_persisted += 1;
            }
            break;
        }
    }

    Ok(stats)
}

async fn process_message<P, Op>(
    message: Message<P>,
    executor: DynExecutor<P, Op>,
    commit_sink: DynCommitSink<Op>,
    deferred_store: DynDeferredStore<P>,
    deferred_tx: mpsc::Sender<Message<P>>,
    metrics: DynRuntimeMetrics,
    epoch_tracker: Option<Arc<super::EpochTracker>>,
) -> Result<MessageRunStats, IngestError>
where
    P: Clone + Send + Sync + 'static,
    Op: Send + Sync + 'static,
{
    let mut stats = MessageRunStats::default();
    if !message.depends_on.is_empty() {
        stats.deferred_from_schedule += 1;
        deferred_tx.send(message).await.map_err(|_| {
            IngestError::Execution("kernel deferred channel closed".to_string())
        })?;
    } else {
        stats.runnable_messages += 1;
        let msg_stats = execute_runnable(
            message,
            Arc::clone(&executor),
            Arc::clone(&commit_sink),
            Arc::clone(&deferred_store),
            deferred_tx.clone(),
            epoch_tracker.clone(),
        )
        .await?;
        stats.deferred_from_execute += msg_stats.deferred_from_execute;
        stats.committed_batches += msg_stats.committed_batches;
    }

    metrics.observe_throughput("kernel_message", 1);
    Ok(stats)
}

#[derive(Default)]
struct RunnableRunStats {
    deferred_from_execute: usize,
    committed_batches: usize,
}

async fn execute_runnable<P, Op>(
    message: Message<P>,
    executor: DynExecutor<P, Op>,
    commit_sink: DynCommitSink<Op>,
    deferred_store: DynDeferredStore<P>,
    deferred_tx: mpsc::Sender<Message<P>>,
    epoch_tracker: Option<Arc<super::EpochTracker>>,
) -> Result<RunnableRunStats, IngestError>
where
    P: Clone + Send + Sync + 'static,
    Op: Send + Sync + 'static,
{
    let mut stats = RunnableRunStats::default();

    let executor_cloned = Arc::clone(&executor);
    let execute_events = tokio::task::spawn_blocking(move || executor_cloned.execute(message))
        .await
        .map_err(|e| IngestError::Execution(format!("execute join failure: {e}")))??;

    let mut by_epoch: BTreeMap<u64, Vec<ExecutionResult<Op>>> = BTreeMap::new();
    for event in execute_events {
        match event {
            PipelineEvent::Executed { epoch, result } => {
                by_epoch.entry(epoch).or_default().push(result);
            }
            PipelineEvent::Deferred(msg) => {
                if let Some(tracker) = epoch_tracker.as_ref() {
                    tracker.record_internal_submit(msg.epoch)?;
                }
                stats.deferred_from_execute += 1;
                deferred_tx.send(msg).await.map_err(|_| {
                    IngestError::Execution("kernel deferred channel closed".to_string())
                })?;
            }
            PipelineEvent::Fatal { msg_id, error } => {
                let emsg = error.unwrap_or_else(|| "unknown fatal error".to_string());
                warn!("fatal execute event for {msg_id}: {emsg}");
                return Err(IngestError::Execution(format!(
                    "fatal execute event for {msg_id}: {emsg}"
                )));
            }
            PipelineEvent::Runnable(_) => {
                return Err(IngestError::Execution(
                    "executor emitted invalid event".to_string(),
                ));
            }
        }
    }

    for (epoch, results) in by_epoch {
        let completed_results: Vec<(String, Vec<DependencyRef>)> = results
            .iter()
            .map(|r| (r.msg_id.clone(), r.next_dependencies.clone()))
            .collect();
        let sink = Arc::clone(&commit_sink);
        let committed = tokio::task::spawn_blocking(move || sink.commit_epoch(epoch, results))
            .await
            .map_err(|e| IngestError::Execution(format!("commit join failure: {e}")))??;
        stats.committed_batches += committed;

        for (msg_id, next_dependencies) in completed_results {
            notify_dependency_ready(
                Arc::clone(&deferred_store),
                DependencyRef::message(msg_id),
            )
            .await?;
            for dep in next_dependencies {
                notify_dependency_ready(Arc::clone(&deferred_store), dep).await?;
            }
            if let Some(tracker) = epoch_tracker.as_ref() {
                tracker.mark_committed(epoch)?;
            }
        }
    }

    Ok(stats)
}

async fn persist_deferred<P>(
    deferred_store: DynDeferredStore<P>,
    message: Message<P>,
) -> Result<(), IngestError>
where
    P: Clone + Send + Sync + 'static,
{
    tokio::task::spawn_blocking(move || deferred_store.push(message))
        .await
        .map_err(|e| IngestError::Execution(format!("deferred sink join failure: {e}")))?
}

async fn pop_ready_messages<P>(
    deferred_store: DynDeferredStore<P>,
    limit: usize,
) -> Result<Vec<Message<P>>, IngestError>
where
    P: Clone + Send + Sync + 'static,
{
    tokio::task::spawn_blocking(move || deferred_store.pop_ready(limit.max(1)))
        .await
        .map_err(|e| IngestError::Execution(format!("deferred replay join failure: {e}")))?
}

async fn notify_dependency_ready<P>(
    deferred_store: DynDeferredStore<P>,
    dependency: DependencyRef,
) -> Result<(), IngestError>
where
    P: Clone + Send + Sync + 'static,
{
    tokio::task::spawn_blocking(move || {
        deferred_store.notify_ready(crate::types::DependencyReadyEvent { dependency })
    })
    .await
    .map_err(|e| IngestError::Execution(format!("deferred notify join failure: {e}")))?
}
