use std::path::{Path, PathBuf};
use std::process::Command;

/// Locates the JDK core asset (e.g., `lib/modules` or `rt.jar`).
pub fn find_jdk_asset() -> Option<PathBuf> {
    // 1. Try JAVA_HOME environment variable
    if let Ok(home) = std::env::var("JAVA_HOME") {
        if let Some(asset) = check_jdk_home(Path::new(&home)) {
            return Some(asset);
        }
    }

    // 2. Try macOS specific java_home utility
    #[cfg(target_os = "macos")]
    {
        if let Ok(output) = Command::new("/usr/libexec/java_home").output() {
            if output.status.success() {
                let path_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !path_str.is_empty() {
                    if let Some(asset) = check_jdk_home(Path::new(&path_str)) {
                        return Some(asset);
                    }
                }
            }
        }
    }

    // 3. Try generic java command
    if let Ok(output) = Command::new("java")
        .arg("-XshowSettings:properties")
        .arg("-version")
        .output()
    {
        // Output is on stderr usually
        let stderr = String::from_utf8_lossy(&output.stderr);
        for line in stderr.lines() {
            if line.trim().starts_with("java.home = ") {
                let path_str = line.trim().trim_start_matches("java.home = ").trim();
                if let Some(asset) = check_jdk_home(Path::new(path_str)) {
                    return Some(asset);
                }
            }
        }
    }

    None
}

fn check_jdk_home(home: &Path) -> Option<PathBuf> {
    // Check for JImage (Java 9+)
    let modules = home.join("lib").join("modules");
    if modules.exists() {
        return Some(modules);
    }

    // Check for rt.jar (Java 8)
    let rt = home.join("lib").join("rt.jar");
    if rt.exists() {
        return Some(rt);
    }

    // Some JRE layouts
    let jre_rt = home.join("jre").join("lib").join("rt.jar");
    if jre_rt.exists() {
        return Some(jre_rt);
    }

    None
}
