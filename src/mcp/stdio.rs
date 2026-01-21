use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use rmcp::{transport::stdio, ServiceExt};
use crate::index::Naviscope;
use crate::mcp::McpServer;

pub async fn run_stdio_server(
    engine: Arc<RwLock<Option<Naviscope>>>,
    _root_path: Option<PathBuf>, // Not used anymore, kept for API compatibility
) -> Result<(), Box<dyn std::error::Error>> {
    let service = McpServer::new(engine).serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
