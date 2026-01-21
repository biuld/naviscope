use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use axum::{routing::get, Router, extract::State, extract::ws::{WebSocket, WebSocketUpgrade, Message}};
use rmcp::ServiceExt;
use crate::index::Naviscope;
use crate::mcp::McpServer;
use tower_lsp::Client;
use tower_lsp::lsp_types::MessageType;
use tokio_util::sync::CancellationToken;
use tracing::info;
use futures::{sink::SinkExt, stream::StreamExt};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub fn spawn_http_server(
    client: Client,
    engine: Arc<RwLock<Option<Naviscope>>>,
    root_path: PathBuf,
    session_path_lock: Arc<RwLock<Option<PathBuf>>>,
    client_name: Option<String>,
    cancel_token: CancellationToken,
) {
    tokio::spawn(async move {
        let port = {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.ok();
            listener.map(|l| l.local_addr().unwrap().port())
        };

        if let Some(port) = port {
            // 1. Session recording
            let session_path = super::get_session_path(&root_path);
            let info = super::SessionInfo {
                port,
                pid: std::process::id(),
                root_path: root_path.clone(),
            };
            if let Ok(json) = serde_json::to_string(&info) {
                let _ = std::fs::write(&session_path, json);
                let mut lock = session_path_lock.write().await;
                *lock = Some(session_path);
            }

            // 2. Auto-config for Cursor
            if let Some(name) = &client_name {
                if name.to_lowercase().contains("cursor") {
                    write_cursor_config(&root_path);
                }
            }

            // 3. Run server
            let mcp_err = match run_http_server(engine, Some(root_path), port, cancel_token).await {
                Ok(_) => None,
                Err(e) => Some(e.to_string()),
            };
            if let Some(e) = mcp_err {
                let _ = client.log_message(MessageType::ERROR, format!("MCP HTTP Server failed: {}", e)).await;
            }
        }
    });
}

fn write_cursor_config(root_path: &Path) {
    let config = serde_json::json!({
        "mcpServers": {
            "naviscope": {
                "command": "naviscope",
                "args": ["mcp", "--path", root_path.to_string_lossy()]
            }
        }
    });
    let dot_cursor = root_path.join(".cursor");
    let _ = std::fs::create_dir_all(&dot_cursor);
    let _ = std::fs::write(
        dot_cursor.join("mcp.json"),
        serde_json::to_string_pretty(&config).unwrap_or_default(),
    );
}

pub async fn run_http_server(
    engine: Arc<RwLock<Option<Naviscope>>>,
    _root_path: Option<PathBuf>, // Kept for API compatibility, but not used in McpServer
    port: u16,
    cancel_token: CancellationToken,
) -> Result<(), Box<dyn std::error::Error>> {
    let mcp = McpServer::new(engine);
    
    let app = Router::new()
        .route("/mcp", get(mcp_ws_handler))
        .with_state(mcp);

    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{}", port)).await?;
    info!("MCP WebSocket server listening on 127.0.0.1:{}", port);
    
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            cancel_token.cancelled().await;
        })
        .await?;
    Ok(())
}

async fn mcp_ws_handler(
    ws: WebSocketUpgrade,
    State(mcp): State<McpServer>,
) -> impl axum::response::IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, mcp))
}

async fn handle_socket(socket: WebSocket, mcp: McpServer) {
    let (mut ws_sink, mut ws_stream) = socket.split();
    
    // Create a duplex pair to bridge WebSocket with McpServer
    let (client_end, server_end) = tokio::io::duplex(4096);
    let (mut client_reader, mut client_writer) = tokio::io::split(client_end);

    // Task 1: Forward WebSocket -> McpServer
    let mut ws_to_mcp = tokio::spawn(async move {
        while let Some(Ok(msg)) = ws_stream.next().await {
            match msg {
                Message::Binary(data) => {
                    if client_writer.write_all(&data).await.is_err() {
                        break;
                    }
                    let _ = client_writer.flush().await;
                }
                Message::Text(data) => {
                    if client_writer.write_all(data.as_bytes()).await.is_err() {
                        break;
                    }
                    let _ = client_writer.flush().await;
                }
                Message::Close(_) => break,
                _ => {}
            }
        }
    });

    // Task 2: Forward McpServer -> WebSocket
    let mut mcp_to_ws = tokio::spawn(async move {
        let mut buf = vec![0u8; 4096];
        while let Ok(n) = client_reader.read(&mut buf).await {
            if n == 0 { break; }
            if ws_sink.send(Message::Binary(buf[..n].to_vec().into())).await.is_err() {
                break;
            }
        }
    });

    // Task 3: Run MCP service
    tokio::spawn(async move {
        if let Ok(service) = mcp.serve(server_end).await {
            let _ = service.waiting().await;
        }
    });

    // Wait for either direction to close
    tokio::select! {
        _ = (&mut ws_to_mcp) => { mcp_to_ws.abort(); },
        _ = (&mut mcp_to_ws) => { ws_to_mcp.abort(); },
    }
}
