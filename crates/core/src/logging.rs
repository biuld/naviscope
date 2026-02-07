use std::path::Path;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

pub fn init_logging(component: &str, to_stderr: bool) -> WorkerGuard {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let log_dir = Path::new(&home).join(".naviscope/logs");
    let _ = std::fs::create_dir_all(&log_dir);

    // Roll daily, with the component name as the prefix
    // This will create files like lsp.log.2024-01-21
    let file_appender = tracing_appender::rolling::daily(&log_dir, component);
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    // File layer: no ANSI colors, output to file
    let file_layer = fmt::layer()
        .with_writer(non_blocking)
        .with_ansi(false)
        .with_target(true);

    let registry = tracing_subscriber::registry().with(filter).with(file_layer);

    if to_stderr {
        let stderr_layer = fmt::layer()
            .with_writer(std::io::stderr)
            .with_ansi(true)
            .with_target(false);
        registry.with(stderr_layer).init();
    } else {
        registry.init();
    }

    guard
}
