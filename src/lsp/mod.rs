pub mod capabilities;
pub mod goto;
pub mod hierarchy;
pub mod hover;
pub mod symbols;
pub mod util;

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};
use crate::index::Naviscope;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct Backend {
    client: Client,
    pub naviscope: Arc<RwLock<Option<Naviscope>>>,
}

impl Backend {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            naviscope: Arc::new(RwLock::new(None)),
        }
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        let root_path = params.root_uri.and_then(|uri| uri.to_file_path().ok());
        
        if let Some(path) = root_path {
            let naviscope_lock = self.naviscope.clone();
            let client = self.client.clone();
            
            // Initial indexing in background
            tokio::spawn(async move {
                client.log_message(MessageType::INFO, format!("Naviscope indexing started for {:?}", path)).await;
                let mut navi = Naviscope::new(path);
                if let Err(e) = navi.build_index() {
                    client.log_message(MessageType::ERROR, format!("Indexing failed: {}", e)).await;
                } else {
                    client.log_message(MessageType::INFO, "Naviscope indexing complete").await;
                    let mut lock = naviscope_lock.write().await;
                    *lock = Some(navi);
                }
            });
        }

        Ok(InitializeResult {
            server_info: Some(ServerInfo {
                name: "Naviscope".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
            capabilities: capabilities::server_capabilities(),
        })
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        hover::handle(self, params).await
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        goto::definition(self, params).await
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        goto::references(self, params).await
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        symbols::document_symbol(self, params).await
    }

    async fn symbol(
        &self,
        params: WorkspaceSymbolParams,
    ) -> Result<Option<Vec<SymbolInformation>>> {
        symbols::workspace_symbol(self, params).await
    }

    async fn goto_implementation(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        goto::implementation(self, params).await
    }

    async fn goto_type_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        goto::type_definition(self, params).await
    }

    async fn document_highlight(
        &self,
        params: DocumentHighlightParams,
    ) -> Result<Option<Vec<DocumentHighlight>>> {
        goto::document_highlight(self, params).await
    }

    async fn prepare_call_hierarchy(
        &self,
        params: CallHierarchyPrepareParams,
    ) -> Result<Option<Vec<CallHierarchyItem>>> {
        hierarchy::prepare_call_hierarchy(self, params).await
    }

    async fn incoming_calls(
        &self,
        params: CallHierarchyIncomingCallsParams,
    ) -> Result<Option<Vec<CallHierarchyIncomingCall>>> {
        hierarchy::incoming_calls(self, params).await
    }

    async fn outgoing_calls(
        &self,
        params: CallHierarchyOutgoingCallsParams,
    ) -> Result<Option<Vec<CallHierarchyOutgoingCall>>> {
        hierarchy::outgoing_calls(self, params).await
    }
}

pub async fn run_server() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = tower_lsp::LspService::new(|client| Backend::new(client));
    tower_lsp::Server::new(stdin, stdout, socket).serve(service).await;

    Ok(())
}
