//! Asset/Stub Layer - Independent module for asset discovery and stub generation.
//!
//! This module provides the core infrastructure for:
//! - Discovering assets (JARs, JDK modules) from various sources
//! - Building route tables (FQN prefix -> asset paths)
//! - Managing stub requests and generation
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────┐    ┌───────────────────────┐
//! │   AssetDiscoverer[]     │    │   AssetIndexer[]      │
//! │   (JDK, Gradle, Maven)  │───▶│   (extract prefixes)  │
//! └─────────────────────────┘    └───────────┬───────────┘
//!                                            │
//!                                            ▼
//!                            ┌───────────────────────────┐
//!                            │   AssetRouteRegistry      │
//!                            │   (prefix → paths)        │
//!                            └───────────────────────────┘
//! ```
//!
//! ## Note
//!
//! Concrete discoverers are implemented in their respective crates:
//! - `naviscope-java::JdkDiscoverer` - JDK asset discovery
//! - `naviscope-gradle::GradleCacheDiscoverer` - Gradle cache discovery

pub mod registry;
pub mod scanner;
pub mod service;

pub use registry::InMemoryRouteRegistry;
pub use scanner::AssetScanner;
pub use service::AssetStubService;
