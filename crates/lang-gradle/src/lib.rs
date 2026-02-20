pub mod cap;
pub mod discoverer;
pub mod model;
pub mod parser;
pub mod queries;
pub mod resolve;

pub use cap::gradle_caps;
pub use discoverer::GradleCacheDiscoverer;

pub struct GradlePlugin {
    _private: (),
}

impl GradlePlugin {
    pub fn new() -> Self {
        Self { _private: () }
    }
}
