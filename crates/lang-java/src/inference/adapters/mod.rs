//! Adapters that implement JavaTypeSystem for various data sources.

mod graph;
mod heuristic;

pub use graph::CodeGraphTypeSystem;
pub use heuristic::HeuristicAdapter;
