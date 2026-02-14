use std::collections::BTreeMap;

pub type MessageId = String;
pub type Topic = String;
pub type MessageGroup = String;
pub type ResourceKey = String;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DependencyKind {
    Message,
    Resource,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DependencyRef {
    pub kind: DependencyKind,
    pub target: String,
    pub min_version: Option<u64>,
}

impl DependencyRef {
    pub fn message(msg_id: impl Into<String>) -> Self {
        Self {
            kind: DependencyKind::Message,
            target: msg_id.into(),
            min_version: None,
        }
    }

    pub fn resource(resource_key: impl Into<String>, min_version: Option<u64>) -> Self {
        Self {
            kind: DependencyKind::Resource,
            target: resource_key.into(),
            min_version,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Message<P = Vec<u8>> {
    pub msg_id: MessageId,
    pub topic: Topic,
    pub message_group: MessageGroup,
    pub version: u64,
    pub depends_on: Vec<DependencyRef>,
    pub epoch: u64,
    pub payload: P,
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionStatus {
    Done,
    Deferred,
    RetryableError,
    FatalError,
}

#[derive(Debug, Clone)]
pub struct ExecutionResult<Op> {
    pub msg_id: MessageId,
    pub status: ExecutionStatus,
    pub operations: Vec<Op>,
    pub next_dependencies: Vec<DependencyRef>,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct OperationBatch<Op> {
    pub batch_id: String,
    pub epoch: u64,
    pub operations: Vec<Op>,
    pub source_msgs: Vec<MessageId>,
}

#[derive(Debug, Clone)]
pub struct DependencyReadyEvent {
    pub dependency: DependencyRef,
}

#[derive(Debug, Clone)]
pub enum PipelineEvent<P, Op> {
    Runnable(Message<P>),
    Deferred(Message<P>),
    Executed {
        epoch: u64,
        result: ExecutionResult<Op>,
    },
    Fatal {
        msg_id: MessageId,
        error: Option<String>,
    },
}

#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub deferred_poll_limit: usize,
    pub kernel_channel_capacity: usize,
    pub max_in_flight: usize,
    pub idle_sleep_ms: u64,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            deferred_poll_limit: 256,
            kernel_channel_capacity: 256,
            max_in_flight: 256,
            idle_sleep_ms: 10,
        }
    }
}
