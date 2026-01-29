use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher as NotifyWatcher};
use std::path::Path;
use tokio::sync::mpsc;

pub struct Watcher {
    // Keep watcher alive
    _watcher: RecommendedWatcher,
    pub(crate) rx: mpsc::UnboundedReceiver<notify::Result<Event>>,
}

impl Watcher {
    pub fn new(root: &Path) -> notify::Result<Self> {
        let (tx, rx) = mpsc::unbounded_channel();

        let mut watcher = RecommendedWatcher::new(
            move |res| {
                let _ = tx.send(res);
            },
            Config::default(),
        )?;

        // Watch recursively
        watcher.watch(root, RecursiveMode::Recursive)?;

        Ok(Self {
            _watcher: watcher,
            rx,
        })
    }

    /// Blocks until an event is received.
    pub fn next_event(&mut self) -> Option<Event> {
        match self.rx.blocking_recv() {
            Some(Ok(event)) => Some(event),
            _ => None,
        }
    }

    /// Tries to receive an event without blocking.
    pub fn try_next_event(&mut self) -> Option<Event> {
        match self.rx.try_recv() {
            Ok(Ok(event)) => Some(event),
            _ => None,
        }
    }
}
