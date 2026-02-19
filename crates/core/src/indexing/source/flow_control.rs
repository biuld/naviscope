#[derive(Clone, Copy)]
pub(super) struct SourceFlowControl {
    pub(super) max_parallelism: usize,
    pub(super) collect_cache_limit: usize,
    pub(super) analyze_cache_limit: usize,
}

impl Default for SourceFlowControl {
    fn default() -> Self {
        let max_parallelism = std::env::var("NAVISCOPE_SOURCE_MAX_PARALLELISM")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .filter(|v| *v > 0)
            .unwrap_or_else(|| std::thread::available_parallelism().map_or(4, usize::from));
        let collect_cache_limit = std::env::var("NAVISCOPE_SOURCE_COLLECT_CACHE_LIMIT")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .filter(|v| *v > 0)
            .unwrap_or(512);
        let analyze_cache_limit = std::env::var("NAVISCOPE_SOURCE_ANALYZE_CACHE_LIMIT")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .filter(|v| *v > 0)
            .unwrap_or(512);

        Self {
            max_parallelism,
            collect_cache_limit,
            analyze_cache_limit,
        }
    }
}
