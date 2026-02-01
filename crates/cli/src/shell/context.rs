use naviscope_api::NaviscopeEngine;
use naviscope_api::graph::GraphService;
use naviscope_api::models::{GraphQuery, QueryResult};
use naviscope_api::navigation::NavigationService;
use std::sync::{Arc, RwLock};

#[derive(Clone)]
pub struct ShellContext {
    pub engine: Arc<dyn NaviscopeEngine>,
    pub rt_handle: tokio::runtime::Handle,
    pub current_node: Arc<RwLock<Option<String>>>,
}

// Re-export ResolveResult from API
pub use naviscope_api::navigation::ResolveResult;

impl ShellContext {
    pub fn new(
        engine: Arc<dyn NaviscopeEngine>,
        rt_handle: tokio::runtime::Handle,
        current_node: Arc<RwLock<Option<String>>>,
    ) -> Self {
        Self {
            engine,
            rt_handle,
            current_node,
        }
    }

    pub fn current_fqn(&self) -> Option<String> {
        self.current_node.read().unwrap().clone()
    }

    pub fn set_current_fqn(&self, fqn: Option<String>) {
        *self.current_node.write().unwrap() = fqn;
    }

    /// Helper to execute query synchronously using the API GraphService
    pub fn execute_query(
        &self,
        query: &GraphQuery,
    ) -> Result<QueryResult, Box<dyn std::error::Error>> {
        let service: &dyn GraphService = self.engine.as_ref();
        let result = if tokio::runtime::Handle::try_current().is_ok() {
            tokio::task::block_in_place(|| self.rt_handle.block_on(service.query(query)))
        } else {
            self.rt_handle.block_on(service.query(query))
        };
        Ok(result?)
    }

    /// Resolves a user input path using the NavigationService API.
    pub fn resolve_node(&self, target: &str) -> ResolveResult {
        let nav_service: &dyn NavigationService = self.engine.as_ref();
        let current_context = self.current_fqn();

        if tokio::runtime::Handle::try_current().is_ok() {
            tokio::task::block_in_place(|| {
                self.rt_handle
                    .block_on(nav_service.resolve_path(target, current_context.as_deref()))
            })
        } else {
            self.rt_handle
                .block_on(nav_service.resolve_path(target, current_context.as_deref()))
        }
    }
}
