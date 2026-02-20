//! Adapters that implement JavaTypeSystem for various data sources.

mod graph;
mod heuristic;
mod noop;

pub use graph::CodeGraphTypeSystem;
pub use heuristic::HeuristicAdapter;
pub use noop::NoOpTypeSystem;
