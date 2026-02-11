use crate::JavaPlugin;
use naviscope_plugin::{AssetCap, AssetDiscoverer, AssetIndexer, AssetSourceLocator};
use std::sync::Arc;

impl AssetCap for JavaPlugin {
    fn global_asset_discoverer(&self) -> Option<Box<dyn AssetDiscoverer>> {
        Some(Box::new(crate::discoverer::JdkDiscoverer::new()))
    }

    fn asset_indexer(&self) -> Option<Arc<dyn AssetIndexer>> {
        Some(Arc::new(crate::resolve::external::JavaExternalResolver))
    }

    fn asset_source_locator(&self) -> Option<Arc<dyn AssetSourceLocator>> {
        Some(Arc::new(crate::resolve::external::JavaExternalResolver))
    }

    fn stub_generator(&self) -> Option<Arc<dyn naviscope_plugin::StubGenerator>> {
        Some(Arc::new(crate::resolve::external::JavaExternalResolver))
    }
}
