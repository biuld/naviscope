use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use axum::{routing::post, Router, body::Body, extract::State};
use rmcp::ServiceExt;
use crate::index::Naviscope;
use crate::mcp::McpServer;
use tower_lsp::Client;
use tower_lsp::lsp_types::MessageType;

pub fn spawn_http_server(
    client: Client,
    engine: Arc<RwLock<Option<Naviscope>>>,
    root_path: PathBuf,
    session_path_lock: Arc<RwLock<Option<PathBuf>>>,
    client_name: Option<String>,
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
            if let Some(name) = client_name {
                if name.to_lowercase().contains("cursor") {
                    write_cursor_config(&root_path);
                }
            }

            // 3. Run server
            let mcp_err = match run_http_server(engine, Some(root_path), port).await {
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
    let _ = std::fs::write(dot_cursor.join("mcp.json"), serde_json::to_string_pretty(&config).unwrap());
}

pub async fn run_http_server(
    engine: Arc<RwLock<Option<Naviscope>>>,
    root_path: Option<PathBuf>,
    port: u16,
) -> Result<(), Box<dyn std::error::Error>> {
    let mcp = McpServer::new(engine, root_path);
    
    let app = Router::new()
        .route("/mcp", post(mcp_handler))
        .with_state(mcp);

    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{}", port)).await?;
    println!("MCP HTTP server listening on 127.0.0.1:{}", port);
    axum::serve(listener, app).await?;
    Ok(())
}

async fn mcp_handler(
    State(mcp): State<McpServer>,
    req: axum::extract::Request,
) -> impl axum::response::IntoResponse {
    use tokio_util::io::{StreamReader, ReaderStream};
    use futures::StreamExt;
    
    let body = req.into_body();
    let read_stream = body.into_data_stream().map(|res| {
        res.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
    });
    let reader = StreamReader::new(read_stream);
    
    let (client_end, server_end) = tokio::io::duplex(4096);
    
    tokio::spawn(async move {
        let transport = (reader, server_end);
        if let Ok(service) = mcp.serve(transport).await {
            let _ = service.waiting().await;
        }
    });

    axum::response::Response::builder()
        .header("content-type", "application/octet-stream")
        .body(Body::from_stream(ReaderStream::new(client_end)))
        .unwrap()
}
