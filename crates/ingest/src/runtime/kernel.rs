use std::collections::BTreeMap;
use std::sync::Arc;

use rayon::prelude::*;
use tokio::sync::mpsc;
use tracing::warn;

use crate::error::IngestError;
use crate::traits::{CommitSink, DeferredStore, Executor, RuntimeMetrics, Scheduler};
use crate::types::{ExecutionResult, Message, PipelineEvent, RuntimeConfig};

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

#[derive(Clone)]
pub struct KernelConfig {
    pub channel_capacity: usize,
    pub schedule_batch_size: usize,
    pub execute_batch_size: usize,
    pub idle_sleep_ms: u64,
}

impl Default for KernelConfig {
    fn default() -> Self {
        Self {
            channel_capacity: 256,
            schedule_batch_size: 256,
            execute_batch_size: 256,
            idle_sleep_ms: 10,
        }
    }
}

impl From<&RuntimeConfig> for KernelConfig {
    fn from(value: &RuntimeConfig) -> Self {
        Self {
            channel_capacity: value.kernel_channel_capacity,
            schedule_batch_size: value.schedule_batch_size,
            execute_batch_size: value.execute_batch_size,
            idle_sleep_ms: value.idle_sleep_ms,
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

pub async fn run_pipeline<P, Op, SCH, EX, DS, C, RM>(
    channels: BusChannels<P>,
    scheduler: Arc<SCH>,
    executor: Arc<EX>,
    deferred_store: Arc<DS>,
    commit_sink: Arc<C>,
    metrics: Arc<RM>,
    config: &KernelConfig,
) -> Result<KernelRunStats, IngestError>
where
    P: Clone + Send + Sync + 'static,
    Op: Send + Sync + 'static,
    SCH: Scheduler<P, Op> + Send + Sync + 'static + ?Sized,
    EX: Executor<P, Op> + Send + Sync + 'static + ?Sized,
    DS: DeferredStore<P> + Send + Sync + 'static + ?Sized,
    C: CommitSink<Op> + Send + Sync + 'static + ?Sized,
    RM: RuntimeMetrics + Send + Sync + 'static + ?Sized,
{
    let BusChannels {
        intake_tx,
        mut intake_rx,
        deferred_tx,
        mut deferred_rx,
    } = channels;
    drop(intake_tx);

    let deferred_handle = {
        let store = Arc::clone(&deferred_store);
        tokio::spawn(async move {
            let mut persisted = 0usize;
            while let Some(msg) = deferred_rx.recv().await {
                let store_cloned = Arc::clone(&store);
                tokio::task::spawn_blocking(move || store_cloned.push(msg))
                    .await
                    .map_err(|e| {
                        IngestError::Execution(format!("deferred sink join failure: {e}"))
                    })??;
                persisted += 1;
            }
            Ok::<usize, IngestError>(persisted)
        })
    };

    let schedule_batch_size = config.schedule_batch_size.max(1);
    let execute_batch_size = config.execute_batch_size.max(1);
    let mut stats = KernelRunStats::default();
    let mut schedule_batch = Vec::with_capacity(schedule_batch_size);
    let mut execute_batch = Vec::with_capacity(execute_batch_size);

    while let Some(msg) = intake_rx.recv().await {
        schedule_batch.push(msg);
        if schedule_batch.len() < schedule_batch_size {
            continue;
        }

        let scheduler_cloned = Arc::clone(&scheduler);
        let input = std::mem::take(&mut schedule_batch);
        let schedule_events = tokio::task::spawn_blocking(move || scheduler_cloned.schedule(input))
            .await
            .map_err(|e| IngestError::Execution(format!("schedule join failure: {e}")))??;

        for event in schedule_events {
            match event {
                PipelineEvent::Runnable(msg) => {
                    stats.runnable_messages += 1;
                    execute_batch.push(msg);
                }
                PipelineEvent::Deferred(msg) => {
                    stats.deferred_from_schedule += 1;
                    deferred_tx.send(msg).await.map_err(|_| {
                        IngestError::Execution("kernel deferred channel closed".to_string())
                    })?;
                }
                _ => {
                    return Err(IngestError::Execution(
                        "scheduler emitted invalid event".to_string(),
                    ));
                }
            }
        }

        if execute_batch.len() < execute_batch_size {
            continue;
        }

        let executor_cloned = Arc::clone(&executor);
        let input = std::mem::take(&mut execute_batch);
        let events_by_msg = tokio::task::spawn_blocking(move || {
            let raw: Vec<Result<Vec<PipelineEvent<P, Op>>, IngestError>> =
                input.into_par_iter().map(|m| executor_cloned.execute(m)).collect();
            let mut out = Vec::with_capacity(raw.len());
            for item in raw {
                out.push(item?);
            }
            Ok::<Vec<Vec<PipelineEvent<P, Op>>>, IngestError>(out)
        })
        .await
        .map_err(|e| IngestError::Execution(format!("execute join failure: {e}")))??;

        let mut by_epoch: BTreeMap<u64, Vec<ExecutionResult<Op>>> = BTreeMap::new();
        for events in events_by_msg {
            for event in events {
                match event {
                    PipelineEvent::Executed { epoch, result } => {
                        by_epoch.entry(epoch).or_default().push(result);
                    }
                    PipelineEvent::Deferred(msg) => {
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
        }

        for (epoch, results) in by_epoch {
            let sink = Arc::clone(&commit_sink);
            let committed = tokio::task::spawn_blocking(move || sink.commit_epoch(epoch, results))
                .await
                .map_err(|e| IngestError::Execution(format!("commit join failure: {e}")))??;
            stats.committed_batches += committed;
        }
    }

    if !schedule_batch.is_empty() {
        let scheduler_cloned = Arc::clone(&scheduler);
        let input = std::mem::take(&mut schedule_batch);
        let schedule_events = tokio::task::spawn_blocking(move || scheduler_cloned.schedule(input))
            .await
            .map_err(|e| IngestError::Execution(format!("schedule join failure: {e}")))??;

        for event in schedule_events {
            match event {
                PipelineEvent::Runnable(msg) => {
                    stats.runnable_messages += 1;
                    execute_batch.push(msg);
                }
                PipelineEvent::Deferred(msg) => {
                    stats.deferred_from_schedule += 1;
                    deferred_tx.send(msg).await.map_err(|_| {
                        IngestError::Execution("kernel deferred channel closed".to_string())
                    })?;
                }
                _ => {
                    return Err(IngestError::Execution(
                        "scheduler emitted invalid event".to_string(),
                    ));
                }
            }
        }
    }

    if !execute_batch.is_empty() {
        let executor_cloned = Arc::clone(&executor);
        let input = std::mem::take(&mut execute_batch);
        let events_by_msg = tokio::task::spawn_blocking(move || {
            let raw: Vec<Result<Vec<PipelineEvent<P, Op>>, IngestError>> =
                input.into_par_iter().map(|m| executor_cloned.execute(m)).collect();
            let mut out = Vec::with_capacity(raw.len());
            for item in raw {
                out.push(item?);
            }
            Ok::<Vec<Vec<PipelineEvent<P, Op>>>, IngestError>(out)
        })
        .await
        .map_err(|e| IngestError::Execution(format!("execute join failure: {e}")))??;

        let mut by_epoch: BTreeMap<u64, Vec<ExecutionResult<Op>>> = BTreeMap::new();
        for events in events_by_msg {
            for event in events {
                match event {
                    PipelineEvent::Executed { epoch, result } => {
                        by_epoch.entry(epoch).or_default().push(result);
                    }
                    PipelineEvent::Deferred(msg) => {
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
        }

        for (epoch, results) in by_epoch {
            let sink = Arc::clone(&commit_sink);
            let committed = tokio::task::spawn_blocking(move || sink.commit_epoch(epoch, results))
                .await
                .map_err(|e| IngestError::Execution(format!("commit join failure: {e}")))??;
            stats.committed_batches += committed;
        }
    }

    drop(deferred_tx);
    stats.deferred_persisted = deferred_handle
        .await
        .map_err(|e| IngestError::Execution(format!("kernel deferred join failure: {e}")))??;

    metrics.observe_queue_depth("kernel_runnable_total", stats.runnable_messages);
    metrics.observe_queue_depth(
        "kernel_deferred_total",
        stats.deferred_from_schedule + stats.deferred_from_execute,
    );
    metrics.observe_queue_depth("kernel_deferred_persisted", stats.deferred_persisted);

    Ok(stats)
}
