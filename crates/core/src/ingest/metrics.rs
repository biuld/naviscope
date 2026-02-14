use naviscope_ingest::RuntimeMetrics;

pub struct NoopRuntimeMetrics;

impl RuntimeMetrics for NoopRuntimeMetrics {
    fn observe_queue_depth(&self, _queue: &'static str, _depth: usize) {}

    fn observe_throughput(&self, _stage: &'static str, _count: usize) {}

    fn observe_latency_ms(&self, _stage: &'static str, _p95_ms: u64, _p99_ms: u64) {}

    fn observe_replay_result(&self, _ok: bool) {}
}
