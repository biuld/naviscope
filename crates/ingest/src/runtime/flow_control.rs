use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use crate::error::IngestError;
use crate::types::RuntimeConfig;

#[derive(Clone)]
pub struct FlowControlConfig {
    pub channel_capacity: usize,
    pub max_in_flight: usize,
    pub deferred_poll_limit: usize,
    pub idle_sleep_ms: u64,
}

impl Default for FlowControlConfig {
    fn default() -> Self {
        Self {
            channel_capacity: 256,
            max_in_flight: 256,
            deferred_poll_limit: 256,
            idle_sleep_ms: 10,
        }
    }
}

impl From<&RuntimeConfig> for FlowControlConfig {
    fn from(value: &RuntimeConfig) -> Self {
        Self {
            channel_capacity: value.kernel_channel_capacity,
            max_in_flight: value.max_in_flight,
            deferred_poll_limit: value.deferred_poll_limit,
            idle_sleep_ms: value.idle_sleep_ms,
        }
    }
}

#[derive(Clone)]
pub struct FlowController {
    in_flight: Arc<Semaphore>,
    deferred_poll_limit: usize,
    idle_sleep: Duration,
}

impl FlowController {
    pub fn new(config: &FlowControlConfig) -> Self {
        Self {
            in_flight: Arc::new(Semaphore::new(config.max_in_flight.max(1))),
            deferred_poll_limit: config.deferred_poll_limit.max(1),
            idle_sleep: Duration::from_millis(config.idle_sleep_ms.max(1)),
        }
    }

    pub async fn acquire_in_flight(&self) -> Result<OwnedSemaphorePermit, IngestError> {
        self.in_flight
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| IngestError::Execution("in-flight flow controller closed".to_string()))
    }

    pub fn deferred_poll_limit(&self) -> usize {
        self.deferred_poll_limit
    }

    pub fn idle_sleep(&self) -> Duration {
        self.idle_sleep
    }
}
