use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use naviscope_ingest::runtime::kernel;
use naviscope_ingest::{
    CommitSink, DeferredStore, Executor, FlowControlConfig, IngestError, IngestRuntime,
    PipelineBus, PipelineEvent, RuntimeComponents, RuntimeConfig, RuntimeMetrics, TokioPipelineBus,
};
use naviscope_ingest::{
    DependencyReadyEvent, DependencyRef, ExecutionResult, ExecutionStatus, Message,
};

fn message(id: &str, epoch: u64, payload: u8) -> Message<u8> {
    Message {
        msg_id: id.to_string(),
        topic: "t".to_string(),
        message_group: "g".to_string(),
        version: 1,
        depends_on: vec![],
        epoch,
        payload,
        metadata: BTreeMap::new(),
    }
}

struct TestExecutor;
impl Executor<u8, String> for TestExecutor {
    fn execute(&self, message: Message<u8>) -> Result<Vec<PipelineEvent<u8, String>>, IngestError> {
        let event = match message.payload {
            2 => PipelineEvent::Deferred(message),
            3 => PipelineEvent::Fatal {
                msg_id: message.msg_id,
                error: Some("fatal".to_string()),
            },
            _ => PipelineEvent::Executed {
                epoch: message.epoch,
                result: ExecutionResult {
                    msg_id: message.msg_id,
                    status: ExecutionStatus::Done,
                    operations: vec!["op".to_string()],
                    next_dependencies: vec![],
                    error: None,
                },
            },
        };
        Ok(vec![event])
    }
}

#[derive(Default)]
struct TestDeferredStore {
    pushed: Mutex<Vec<String>>,
    notified: Mutex<Vec<String>>,
    ready: Mutex<Vec<Message<u8>>>,
}
impl DeferredStore<u8> for TestDeferredStore {
    fn push(&self, message: Message<u8>) -> Result<(), IngestError> {
        self.pushed
            .lock()
            .expect("lock poisoned")
            .push(message.msg_id);
        Ok(())
    }

    fn pop_ready(&self, limit: usize) -> Result<Vec<Message<u8>>, IngestError> {
        let mut guard = self.ready.lock().expect("lock poisoned");
        let n = limit.min(guard.len());
        Ok(guard.drain(0..n).collect())
    }

    fn notify_ready(&self, event: DependencyReadyEvent) -> Result<(), IngestError> {
        self.notified
            .lock()
            .expect("lock poisoned")
            .push(event.dependency.target);
        Ok(())
    }
}

#[derive(Default)]
struct TestCommitSink {
    commits: Mutex<Vec<(u64, usize)>>,
}
impl CommitSink<String> for TestCommitSink {
    fn commit_epoch(
        &self,
        epoch: u64,
        results: Vec<ExecutionResult<String>>,
    ) -> Result<usize, IngestError> {
        let size = results.len();
        self.commits
            .lock()
            .expect("lock poisoned")
            .push((epoch, size));
        Ok(usize::from(size > 0))
    }
}

struct TestMetrics;
impl RuntimeMetrics for TestMetrics {
    fn observe_queue_depth(&self, _queue: &'static str, _depth: usize) {}
    fn observe_throughput(&self, _stage: &'static str, _count: usize) {}
    fn observe_latency_ms(&self, _stage: &'static str, _p95_ms: u64, _p99_ms: u64) {}
    fn observe_replay_result(&self, _ok: bool) {}
}

struct InvalidEventExecutor;
impl Executor<u8, String> for InvalidEventExecutor {
    fn execute(&self, message: Message<u8>) -> Result<Vec<PipelineEvent<u8, String>>, IngestError> {
        Ok(vec![PipelineEvent::Runnable(message)])
    }
}

#[tokio::test]
async fn kernel_commits_runnable_messages() {
    let executor = Arc::new(TestExecutor);
    let store = Arc::new(TestDeferredStore::default());
    let sink = Arc::new(TestCommitSink::default());
    let metrics = Arc::new(TestMetrics);
    let bus = TokioPipelineBus;
    let channels = <TokioPipelineBus as PipelineBus<u8, String>>::open_channels(&bus, 8);
    let tx = channels.intake_tx.clone();

    tx.send(message("m1", 7, 1))
        .await
        .expect("send should work");
    drop(tx);

    let stats = kernel::run_pipeline(
        channels,
        executor,
        store,
        sink.clone(),
        metrics,
        &FlowControlConfig {
            channel_capacity: 8,
            max_in_flight: 8,
            deferred_poll_limit: 8,
            idle_sleep_ms: 1,
        },
    )
    .await
    .expect("pipeline should complete");

    assert_eq!(stats.runnable_messages, 1);
    assert_eq!(stats.committed_batches, 1);
    assert_eq!(
        sink.commits.lock().expect("lock poisoned").as_slice(),
        &[(7, 1)]
    );
}

#[tokio::test]
async fn kernel_persists_deferred_from_both_paths() {
    let executor = Arc::new(TestExecutor);
    let store = Arc::new(TestDeferredStore::default());
    let sink = Arc::new(TestCommitSink::default());
    let metrics = Arc::new(TestMetrics);
    let bus = TokioPipelineBus;
    let channels = <TokioPipelineBus as PipelineBus<u8, String>>::open_channels(&bus, 8);
    let tx = channels.intake_tx.clone();

    let mut sched_deferred = message("m_sched_deferred", 1, 1);
    sched_deferred
        .depends_on
        .push(DependencyRef::message("dep_not_ready"));
    tx.send(sched_deferred)
        .await
        .expect("send should work");
    tx.send(message("m_exec_deferred", 1, 2))
        .await
        .expect("send should work");
    drop(tx);

    let stats = kernel::run_pipeline(
        channels,
        executor,
        store.clone(),
        sink,
        metrics,
        &FlowControlConfig {
            channel_capacity: 8,
            max_in_flight: 8,
            deferred_poll_limit: 8,
            idle_sleep_ms: 1,
        },
    )
    .await
    .expect("pipeline should complete");

    assert_eq!(stats.deferred_from_schedule, 1);
    assert_eq!(stats.deferred_from_execute, 1);
    assert_eq!(stats.deferred_persisted, 2);
    let pushed = store.pushed.lock().expect("lock poisoned").clone();
    assert!(pushed.contains(&"m_sched_deferred".to_string()));
    assert!(pushed.contains(&"m_exec_deferred".to_string()));
}

#[tokio::test]
async fn runtime_notify_dependency_ready_delegates_to_store() {
    let store = Arc::new(TestDeferredStore::default());
    let runtime = IngestRuntime::new(
        RuntimeConfig::default(),
        RuntimeComponents::with_tokio_bus(
            Arc::new(TestExecutor),
            store.clone(),
            Arc::new(TestCommitSink::default()),
            Arc::new(TestMetrics),
        ),
    );

    runtime
        .notify_dependency_ready(DependencyReadyEvent {
            dependency: DependencyRef::message("dep-1"),
        })
        .await
        .expect("notify should work");

    assert_eq!(
        store.notified.lock().expect("lock poisoned").as_slice(),
        &["dep-1".to_string()]
    );
}

#[tokio::test]
async fn kernel_flushes_partial_batches_on_channel_close() {
    let executor = Arc::new(TestExecutor);
    let store = Arc::new(TestDeferredStore::default());
    let sink = Arc::new(TestCommitSink::default());
    let metrics = Arc::new(TestMetrics);
    let bus = TokioPipelineBus;
    let channels = <TokioPipelineBus as PipelineBus<u8, String>>::open_channels(&bus, 8);
    let tx = channels.intake_tx.clone();

    tx.send(message("m_tail", 9, 1))
        .await
        .expect("send should work");
    drop(tx);

    let stats = kernel::run_pipeline(
        channels,
        executor,
        store,
        sink.clone(),
        metrics,
        &FlowControlConfig {
            channel_capacity: 8,
            max_in_flight: 8,
            deferred_poll_limit: 8,
            idle_sleep_ms: 1,
        },
    )
    .await
    .expect("pipeline should flush tail batch");

    assert_eq!(stats.runnable_messages, 1);
    assert_eq!(stats.committed_batches, 1);
    assert_eq!(
        sink.commits.lock().expect("lock poisoned").as_slice(),
        &[(9, 1)]
    );
}

#[tokio::test]
async fn kernel_errors_on_executor_fatal_event() {
    let executor = Arc::new(TestExecutor);
    let store = Arc::new(TestDeferredStore::default());
    let sink = Arc::new(TestCommitSink::default());
    let metrics = Arc::new(TestMetrics);
    let bus = TokioPipelineBus;
    let channels = <TokioPipelineBus as PipelineBus<u8, String>>::open_channels(&bus, 8);
    let tx = channels.intake_tx.clone();

    tx.send(message("m_fatal", 1, 3))
        .await
        .expect("send should work");
    drop(tx);

    let err = kernel::run_pipeline(
        channels,
        executor,
        store,
        sink,
        metrics,
        &FlowControlConfig {
            channel_capacity: 8,
            max_in_flight: 8,
            deferred_poll_limit: 8,
            idle_sleep_ms: 1,
        },
    )
    .await
    .expect_err("fatal event should fail pipeline");

    let msg = err.to_string();
    assert!(msg.contains("fatal execute event"));
    assert!(msg.contains("m_fatal"));
}

#[tokio::test]
async fn kernel_errors_on_invalid_executor_event() {
    let executor = Arc::new(InvalidEventExecutor);
    let store = Arc::new(TestDeferredStore::default());
    let sink = Arc::new(TestCommitSink::default());
    let metrics = Arc::new(TestMetrics);
    let bus = TokioPipelineBus;
    let channels = <TokioPipelineBus as PipelineBus<u8, String>>::open_channels(&bus, 8);
    let tx = channels.intake_tx.clone();

    tx.send(message("m_bad_exec", 1, 1))
        .await
        .expect("send should work");
    drop(tx);

    let err = kernel::run_pipeline(
        channels,
        executor,
        store,
        sink,
        metrics,
        &FlowControlConfig {
            channel_capacity: 8,
            max_in_flight: 8,
            deferred_poll_limit: 8,
            idle_sleep_ms: 1,
        },
    )
    .await
    .expect_err("invalid executor event should fail pipeline");

    assert!(err.to_string().contains("executor emitted invalid event"));
}
