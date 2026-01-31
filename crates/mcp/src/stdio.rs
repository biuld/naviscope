use crate::McpServer;
use naviscope_core::engine::handle::EngineHandle;
use rmcp::{transport::stdio, ServiceExt};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

pub async fn run_stdio_server(
    engine: Arc<RwLock<Option<EngineHandle>>>,
    _root_path: Option<PathBuf>, // Not used anymore, kept for API compatibility
) -> Result<(), Box<dyn std::error::Error>> {
    let service = McpServer::new(engine).serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
