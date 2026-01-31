use once_cell::sync::Lazy;
use std::collections::HashSet;
use std::path::Path;
use std::sync::{Arc, Mutex};

/// A simple global interning pool for strings and paths to reduce memory usage.
pub struct SymbolPool {
    strings: Mutex<HashSet<Arc<str>>>,
    paths: Mutex<HashSet<Arc<Path>>>,
}

impl SymbolPool {
    pub fn new() -> Self {
        Self {
            strings: Mutex::new(HashSet::new()),
            paths: Mutex::new(HashSet::new()),
        }
    }

    pub fn intern_str(&self, s: &str) -> Arc<str> {
        let mut pool = self.strings.lock().unwrap();
        if let Some(existing) = pool.get(s) {
            existing.clone()
        } else {
            let interned: Arc<str> = Arc::from(s);
            pool.insert(interned.clone());
            interned
        }
    }

    pub fn intern_path(&self, p: &Path) -> Arc<Path> {
        let mut pool = self.paths.lock().unwrap();
        if let Some(existing) = pool.get(p) {
            existing.clone()
        } else {
            let interned: Arc<Path> = Arc::from(p);
            pool.insert(interned.clone());
            interned
        }
    }
}

pub static GLOBAL_POOL: Lazy<SymbolPool> = Lazy::new(SymbolPool::new);
