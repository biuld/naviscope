use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use rmcp::{transport::stdio, ServiceExt};
use crate::index::Naviscope;
use crate::mcp::McpServer;

pub async fn run_stdio_server(
    engine: Arc<RwLock<Option<Naviscope>>>,
    root_path: Option<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    let service = McpServer::new(engine, root_path).serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
