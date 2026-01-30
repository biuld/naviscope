//! Unified index engine for Naviscope
//!
//! This module provides a unified, high-performance indexing engine that supports
//! multiple clients (LSP, MCP, Shell) with the following key features:
//!
//! - **Arc-wrapped immutable data**: Cheap cloning via reference counting
//! - **MVCC (Multi-Version Concurrency Control)**: Non-blocking reads during index updates
//! - **Unified interface**: Single `EngineHandle` for all clients
//! - **Async/Sync dual API**: Seamless integration with different runtimes

pub mod builder;
pub mod engine;
pub mod graph;
pub mod handle;

pub use builder::CodeGraphBuilder;
pub use engine::NaviscopeEngine;
pub use graph::CodeGraph;
pub use handle::EngineHandle;
