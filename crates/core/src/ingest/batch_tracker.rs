use std::collections::HashMap;
use std::sync::Mutex;

use tokio::sync::oneshot;

#[derive(Default)]
pub struct BatchTracker {
    inner: Mutex<BatchTrackerInner>,
}

#[derive(Default)]
struct BatchTrackerInner {
    next_batch: u64,
    msg_to_batch: HashMap<String, u64>,
    batches: HashMap<u64, BatchState>,
}

struct BatchState {
    remaining: usize,
    done_tx: Option<oneshot::Sender<()>>,
}

impl BatchTracker {
    pub fn register_batch(&self, msg_ids: &[String]) -> (u64, oneshot::Receiver<()>) {
        let mut guard = self.inner.lock().expect("batch tracker lock poisoned");
        let batch_id = guard.next_batch;
        guard.next_batch += 1;

        let (tx, rx) = oneshot::channel();
        guard.batches.insert(
            batch_id,
            BatchState {
                remaining: msg_ids.len(),
                done_tx: Some(tx),
            },
        );
        for msg_id in msg_ids {
            guard.msg_to_batch.insert(msg_id.clone(), batch_id);
        }

        (batch_id, rx)
    }

    pub fn mark_done(&self, msg_id: &str) {
        let mut guard = self.inner.lock().expect("batch tracker lock poisoned");
        let Some(batch_id) = guard.msg_to_batch.remove(msg_id) else {
            return;
        };

        let mut should_remove = false;
        if let Some(state) = guard.batches.get_mut(&batch_id) {
            if state.remaining > 0 {
                state.remaining -= 1;
            }
            if state.remaining == 0 {
                if let Some(done_tx) = state.done_tx.take() {
                    let _ = done_tx.send(());
                }
                should_remove = true;
            }
        }

        if should_remove {
            guard.batches.remove(&batch_id);
        }
    }
}
