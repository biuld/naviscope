use crate::JavaPlugin;
use naviscope_plugin::FileMatcherCap;
use std::path::Path;

impl FileMatcherCap for JavaPlugin {
    fn supports_path(&self, path: &Path) -> bool {
        path.extension()
            .and_then(|e| e.to_str())
            .map(|ext| ext.eq_ignore_ascii_case("java"))
            .unwrap_or(false)
    }
}
