use tokio::sync::mpsc::UnboundedSender;

/// A request to asynchronously generate a stub for an external FQN
#[derive(Debug, Clone)]
pub struct StubRequest {
    pub fqn: String,
    pub candidate_paths: Vec<std::path::PathBuf>,
}

pub struct StubbingManager {
    tx: UnboundedSender<StubRequest>,
}

impl StubbingManager {
    pub fn new(tx: UnboundedSender<StubRequest>) -> Self {
        Self { tx }
    }

    pub fn request(&self, fqn: String, candidate_paths: Vec<std::path::PathBuf>) {
        self.send(StubRequest {
            fqn,
            candidate_paths,
        });
    }

    pub fn send(&self, req: StubRequest) {
        let fqn = req.fqn.clone();
        match self.tx.send(req) {
            Ok(_) => tracing::trace!("Sent stub request for {}", fqn),
            Err(e) => tracing::warn!("Failed to send stub request for {}: {}", fqn, e),
        }
    }
}
