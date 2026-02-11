use crate::asset::{AssetDiscoverer, AssetIndexer, AssetSourceLocator, StubGenerator};
use std::path::Path;
use std::sync::Arc;

pub trait AssetCap: Send + Sync {
    fn global_asset_discoverer(&self) -> Option<Box<dyn AssetDiscoverer>> {
        None
    }

    fn project_asset_discoverer(&self, _project_root: &Path) -> Option<Box<dyn AssetDiscoverer>> {
        None
    }

    fn asset_indexer(&self) -> Option<Arc<dyn AssetIndexer>> {
        None
    }

    fn asset_source_locator(&self) -> Option<Arc<dyn AssetSourceLocator>> {
        None
    }

    fn stub_generator(&self) -> Option<Arc<dyn StubGenerator>> {
        None
    }
}
