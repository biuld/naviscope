use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher as NotifyWatcher};
use std::path::Path;
use std::sync::mpsc::{self, Receiver};

pub struct Watcher {
    // Keep watcher alive
    _watcher: RecommendedWatcher,
    rx: Receiver<notify::Result<Event>>,
}

impl Watcher {
    pub fn new(root: &Path) -> notify::Result<Self> {
        let (tx, rx) = mpsc::channel();
        let mut watcher = RecommendedWatcher::new(tx, Config::default())?;

        // Watch recursively
        watcher.watch(root, RecursiveMode::Recursive)?;

        Ok(Self {
            _watcher: watcher,
            rx,
        })
    }

    /// Blocks until an event is received.
    pub fn next_event(&self) -> Option<Event> {
        match self.rx.recv() {
            Ok(Ok(event)) => Some(event),
            _ => None,
        }
    }

    // Add non-blocking try_next?
}
