use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Mutex;

use naviscope_ingest::{
    DeferredStore, DependencyKind, DependencyReadyEvent, DependencyRef, IngestError, Message,
};

pub struct InMemoryDeferredQueue<P> {
    deferred: Mutex<VecDeque<Message<P>>>,
    ready_messages: Mutex<HashSet<String>>,
    ready_resources: Mutex<HashMap<String, u64>>,
}

impl<P> Default for InMemoryDeferredQueue<P> {
    fn default() -> Self {
        Self {
            deferred: Mutex::new(VecDeque::new()),
            ready_messages: Mutex::new(HashSet::new()),
            ready_resources: Mutex::new(HashMap::new()),
        }
    }
}

impl<P> DeferredStore<P> for InMemoryDeferredQueue<P>
where
    P: Clone + Send + Sync + 'static,
{
    fn push(&self, message: Message<P>) -> Result<(), IngestError> {
        let mut queue = self
            .deferred
            .lock()
            .map_err(|_| IngestError::Execution("deferred queue poisoned".to_string()))?;
        queue.push_back(message);
        Ok(())
    }

    fn pop_ready(&self, limit: usize) -> Result<Vec<Message<P>>, IngestError> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let ready_messages = self
            .ready_messages
            .lock()
            .map_err(|_| IngestError::Execution("ready message set poisoned".to_string()))?
            .clone();
        let ready_resources = self
            .ready_resources
            .lock()
            .map_err(|_| IngestError::Execution("ready resource map poisoned".to_string()))?
            .clone();

        let mut queue = self
            .deferred
            .lock()
            .map_err(|_| IngestError::Execution("deferred queue poisoned".to_string()))?;

        let mut out = Vec::new();
        let original_len = queue.len();
        for _ in 0..original_len {
            let Some(mut msg) = queue.pop_front() else {
                break;
            };

            let satisfied = msg
                .depends_on
                .iter()
                .all(|dep| dependency_satisfied(dep, &ready_messages, &ready_resources));

            if satisfied && out.len() < limit {
                msg.depends_on.clear();
                out.push(msg);
            } else {
                queue.push_back(msg);
            }
        }

        Ok(out)
    }

    fn notify_ready(&self, event: DependencyReadyEvent) -> Result<(), IngestError> {
        match event.dependency.kind {
            DependencyKind::Message => {
                let mut ready = self
                    .ready_messages
                    .lock()
                    .map_err(|_| IngestError::Execution("ready message set poisoned".to_string()))?;
                ready.insert(event.dependency.target);
            }
            DependencyKind::Resource => {
                let mut ready = self
                    .ready_resources
                    .lock()
                    .map_err(|_| IngestError::Execution("ready resource map poisoned".to_string()))?;
                let version = event.dependency.min_version.unwrap_or(0);
                ready.entry(event.dependency.target)
                    .and_modify(|v| *v = (*v).max(version))
                    .or_insert(version);
            }
        }
        Ok(())
    }
}

fn dependency_satisfied(
    dep: &DependencyRef,
    ready_messages: &HashSet<String>,
    ready_resources: &HashMap<String, u64>,
) -> bool {
    match dep.kind {
        DependencyKind::Message => ready_messages.contains(&dep.target),
        DependencyKind::Resource => match dep.min_version {
            Some(min) => ready_resources
                .get(&dep.target)
                .is_some_and(|version| *version >= min),
            None => ready_resources.contains_key(&dep.target),
        },
    }
}
