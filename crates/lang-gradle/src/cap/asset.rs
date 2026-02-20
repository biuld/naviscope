use crate::GradlePlugin;
use naviscope_plugin::{AssetCap, AssetDiscoverer};

impl AssetCap for GradlePlugin {
    fn global_asset_discoverer(&self) -> Option<Box<dyn AssetDiscoverer>> {
        Some(Box::new(crate::discoverer::GradleCacheDiscoverer::new()))
    }
}
