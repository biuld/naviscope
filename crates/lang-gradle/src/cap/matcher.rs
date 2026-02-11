use crate::GradlePlugin;
use naviscope_plugin::FileMatcherCap;
use std::path::Path;

impl FileMatcherCap for GradlePlugin {
    fn supports_path(&self, path: &Path) -> bool {
        path.file_name()
            .and_then(|n| n.to_str())
            .map(|file_name| {
                file_name == "build.gradle"
                    || file_name == "build.gradle.kts"
                    || file_name == "settings.gradle"
                    || file_name == "settings.gradle.kts"
            })
            .unwrap_or(false)
    }
}
