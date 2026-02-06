use crate::ingest::resolver::ProjectContext;
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;

/// A request to asynchronously generate a stub for an external FQN
#[derive(Debug, Clone)]
pub struct StubRequest {
    pub fqn: String,
    pub context: Arc<ProjectContext>,
}

pub struct StubbingManager {
    tx: UnboundedSender<StubRequest>,
}

impl StubbingManager {
    pub fn new(tx: UnboundedSender<StubRequest>) -> Self {
        Self { tx }
    }

    pub fn request(&self, fqn: String, context: Arc<ProjectContext>) {
        let _ = self.tx.send(StubRequest { fqn, context });
    }
}
