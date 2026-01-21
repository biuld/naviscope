use std::path::Path;
use futures::StreamExt;
use crate::mcp::{get_session_path, SessionInfo};

pub async fn run_proxy_if_needed(path: &Path) -> bool {
    // 1. Find session file
    let session_path = get_session_path(path);

    if !session_path.exists() {
        return false;
    }

    // 2. Read session and check PID
    let session: SessionInfo = match std::fs::read_to_string(&session_path).ok().and_then(|s| serde_json::from_str(&s).ok()) {
        Some(s) => s,
        None => return false,
    };

    // Check if PID is alive
    if !is_pid_alive(session.pid) {
        let _ = std::fs::remove_file(session_path);
        return false;
    }

    // 3. Start proxy
    if let Err(e) = start_http_proxy(session.port).await {
        eprintln!("Proxy failed: {}. Falling back to standalone mode.", e);
        return false;
    }

    true
}

fn is_pid_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        std::process::Command::new("kill")
            .arg("-0")
            .arg(pid.to_string())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        // Simple fallback for non-unix, might need better implementation
        true 
    }
}

async fn start_http_proxy(port: u16) -> Result<(), Box<dyn std::error::Error>> {
    use tokio::io::{stdin, stdout, AsyncWriteExt};
    use reqwest::Client;
    use tokio_util::io::ReaderStream;

    let client = Client::new();
    let url = format!("http://127.0.0.1:{}/mcp", port);

    let stdin_stream = ReaderStream::new(stdin());
    
    let response = client.post(url)
        .body(reqwest::Body::wrap_stream(stdin_stream))
        .send()
        .await?;

    let mut body_stream = response.bytes_stream();
    let mut stdout = stdout();

    while let Some(chunk) = body_stream.next().await {
        let chunk = chunk?;
        stdout.write_all(&chunk).await?;
        stdout.flush().await?;
    }

    Ok(())
}
