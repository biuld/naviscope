use std::path::Path;

pub trait FileMatcherCap: Send + Sync {
    fn supports_path(&self, path: &Path) -> bool;
}
