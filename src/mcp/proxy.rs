use crate::mcp::{SessionInfo, get_session_path};
use futures::{SinkExt, StreamExt};
use std::path::Path;
use tokio::time::{Duration, sleep, timeout};
use tracing::{info, warn};

pub async fn run_mcp_proxy(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    // 1. Find session file
    let session_path = get_session_path(path);

    // Wait for LSP to start (if session file doesn't exist yet)
    if !session_path.exists() {
        info!("LSP session not found, waiting for LSP server to start...");

        // Wait up to 60 seconds for LSP to start
        let wait_result = timeout(Duration::from_secs(60), async {
            loop {
                if session_path.exists() {
                    break;
                }
                sleep(Duration::from_millis(500)).await;
            }
        })
        .await;

        if wait_result.is_err() {
            return Err(format!(
                "LSP server did not start within 60 seconds. Please ensure the LSP server is running for project: {}",
                path.display()
            ).into());
        }

        info!("LSP session file detected, connecting...");
    }

    // 2. Read session and check PID
    let session: SessionInfo = match std::fs::read_to_string(&session_path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
    {
        Some(s) => s,
        None => {
            return Err(
                format!("Failed to parse session file at {}", session_path.display()).into(),
            );
        }
    };

    // Check if PID is alive
    if !is_pid_alive(session.pid) {
        warn!(
            "LSP process (PID: {}) is not alive, removing stale session file",
            session.pid
        );
        let _ = std::fs::remove_file(session_path);
        return Err(format!("LSP process (PID: {}) is not running", session.pid).into());
    }

    // 3. Start proxy
    info!("Connecting to LSP MCP server at port {}", session.port);
    start_ws_proxy(session.port).await?;

    Ok(())
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

async fn start_ws_proxy(port: u16) -> Result<(), Box<dyn std::error::Error>> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt, stdin, stdout};
    use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};

    let url = format!("ws://127.0.0.1:{}/mcp", port);
    let (ws_stream, _) = connect_async(&url).await?;

    let (mut ws_sink, mut ws_stream) = ws_stream.split();
    let mut stdin = stdin();
    let mut stdout = stdout();

    // Task 1: stdin -> WebSocket
    let mut stdin_to_ws = tokio::spawn(async move {
        let mut buf = vec![0u8; 4096];
        while let Ok(n) = stdin.read(&mut buf).await {
            if n == 0 {
                break;
            }
            let msg = Message::Binary(buf[..n].to_vec().into());
            if ws_sink.send(msg).await.is_err() {
                break;
            }
        }
    });

    // Task 2: WebSocket -> stdout
    let mut ws_to_stdout = tokio::spawn(async move {
        while let Some(Ok(msg)) = ws_stream.next().await {
            match msg {
                Message::Binary(data) => {
                    if stdout.write_all(&data).await.is_err() {
                        break;
                    }
                    let _ = stdout.flush().await;
                }
                Message::Text(data) => {
                    if stdout.write_all(data.as_bytes()).await.is_err() {
                        break;
                    }
                    let _ = stdout.flush().await;
                }
                Message::Close(_) => break,
                _ => {}
            }
        }
    });

    // Wait for either direction to close
    tokio::select! {
        _ = (&mut stdin_to_ws) => { ws_to_stdout.abort(); },
        _ = (&mut ws_to_stdout) => { stdin_to_ws.abort(); },
    }

    Ok(())
}
