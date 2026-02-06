//! Tests for global stub cache

use naviscope_api::models::graph::{EmptyMetadata, NodeKind, NodeSource, ResolutionStatus};
use naviscope_api::models::symbol::NodeId;
use naviscope_core::cache::{AssetKey, GlobalStubCache};
use naviscope_plugin::IndexNode;
use std::sync::Arc;
use tempfile::TempDir;

fn create_test_stub(fqn: &str, name: &str) -> IndexNode {
    IndexNode {
        id: NodeId::Flat(fqn.to_string()),
        name: name.to_string(),
        kind: NodeKind::Class,
        lang: "java".to_string(),
        source: NodeSource::External,
        status: ResolutionStatus::Stubbed,
        location: None,
        metadata: Arc::new(EmptyMetadata),
    }
}

#[test]
fn test_cache_store_and_lookup() {
    let temp = TempDir::new().unwrap();
    let cache = GlobalStubCache::new(temp.path().to_path_buf());

    // Create a fake asset file
    let asset_file = temp.path().join("test.jar");
    std::fs::write(&asset_file, b"fake jar content").unwrap();

    let asset_key = AssetKey::from_path(&asset_file).unwrap();

    // Store a stub
    let stub = create_test_stub("com.example.Foo", "Foo");
    cache.store(&asset_key, &stub);

    // Lookup should return the stub
    let cached = cache.lookup(&asset_key, "com.example.Foo");
    assert!(cached.is_some());
    let cached = cached.unwrap();
    assert_eq!(cached.name, "Foo");
    assert_eq!(cached.lang, "java");
}

#[test]
fn test_cache_miss() {
    let temp = TempDir::new().unwrap();
    let cache = GlobalStubCache::new(temp.path().to_path_buf());

    // Create a fake asset file
    let asset_file = temp.path().join("test.jar");
    std::fs::write(&asset_file, b"fake jar content").unwrap();

    let asset_key = AssetKey::from_path(&asset_file).unwrap();

    // Lookup without storing should return None
    let cached = cache.lookup(&asset_key, "com.example.NotCached");
    assert!(cached.is_none());
}

#[test]
fn test_cache_persistence() {
    let temp = TempDir::new().unwrap();
    let cache_dir = temp.path().join("stub_cache");

    // Create a fake asset file
    let asset_file = temp.path().join("test.jar");
    std::fs::write(&asset_file, b"fake jar content").unwrap();

    let asset_key = AssetKey::from_path(&asset_file).unwrap();

    // Store with first cache instance
    {
        let cache = GlobalStubCache::new(cache_dir.clone());
        let stub = create_test_stub("com.example.Persisted", "Persisted");
        cache.store(&asset_key, &stub);
    }

    // Create new cache instance and lookup
    {
        let cache = GlobalStubCache::new(cache_dir.clone());
        let cached = cache.lookup(&asset_key, "com.example.Persisted");
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().name, "Persisted");
    }
}

#[test]
fn test_cache_invalidation_on_file_change() {
    let temp = TempDir::new().unwrap();
    let cache = GlobalStubCache::new(temp.path().join("cache"));

    // Create first version of asset
    let asset_file = temp.path().join("test.jar");
    std::fs::write(&asset_file, b"version 1").unwrap();
    let key1 = AssetKey::from_path(&asset_file).unwrap();

    // Store a stub
    let stub = create_test_stub("com.example.V1", "V1");
    cache.store(&key1, &stub);

    // "Modify" the file (change content and mtime)
    std::thread::sleep(std::time::Duration::from_millis(10));
    std::fs::write(&asset_file, b"version 2 with different size").unwrap();
    let key2 = AssetKey::from_path(&asset_file).unwrap();

    // Keys should be different (different size/mtime)
    assert_ne!(key1.hash(), key2.hash());

    // Old key should still find the stub
    assert!(cache.lookup(&key1, "com.example.V1").is_some());

    // New key should NOT find the stub (different asset version)
    assert!(cache.lookup(&key2, "com.example.V1").is_none());
}

#[test]
fn test_cache_stats() {
    let temp = TempDir::new().unwrap();
    let cache = GlobalStubCache::new(temp.path().to_path_buf());

    // Create multiple assets
    for i in 0..3 {
        let asset_file = temp.path().join(format!("asset{}.jar", i));
        std::fs::write(&asset_file, format!("content{}", i)).unwrap();
        let key = AssetKey::from_path(&asset_file).unwrap();

        for j in 0..5 {
            let stub = create_test_stub(
                &format!("com.example.Class{}_{}", i, j),
                &format!("Class{}_{}", i, j),
            );
            cache.store(&key, &stub);
        }
    }

    let stats = cache.stats();
    assert_eq!(stats.total_assets, 3);
    assert_eq!(stats.total_entries, 15);
}

#[test]
fn test_cache_clear() {
    let temp = TempDir::new().unwrap();
    let cache = GlobalStubCache::new(temp.path().to_path_buf());

    // Create and populate cache
    let asset_file = temp.path().join("test.jar");
    std::fs::write(&asset_file, b"content").unwrap();
    let key = AssetKey::from_path(&asset_file).unwrap();

    cache.store(&key, &create_test_stub("com.example.Test", "Test"));
    assert!(cache.lookup(&key, "com.example.Test").is_some());

    // Clear cache
    cache.clear().unwrap();

    // After clear, should not find anything
    let cache2 = GlobalStubCache::new(temp.path().to_path_buf());
    assert!(cache2.lookup(&key, "com.example.Test").is_none());
}

#[test]
fn test_cache_with_java_metadata() {
    use naviscope_java::model::JavaIndexMetadata;
    use naviscope_plugin::register_metadata_deserializer;

    let temp = TempDir::new().unwrap();
    let cache_dir = temp.path().join("cache");

    // Register the deserializer (normally done when plugin is initialized)
    register_metadata_deserializer("java", JavaIndexMetadata::deserialize_for_cache);

    // Create a fake asset file
    let asset_file = temp.path().join("test.jar");
    std::fs::write(&asset_file, b"content").unwrap();
    let key = AssetKey::from_path(&asset_file).unwrap();

    // 1. Create a stub with Java-specific metadata
    let java_meta = JavaIndexMetadata::Class {
        modifiers: vec!["public".to_string(), "final".to_string()],
    };

    let stub = IndexNode {
        id: NodeId::Flat("com.example.Foo".to_string()),
        name: "Foo".to_string(),
        kind: NodeKind::Class,
        lang: "java".to_string(),
        source: NodeSource::External,
        status: ResolutionStatus::Stubbed,
        location: None,
        metadata: Arc::new(java_meta),
    };

    // 2. Store in cache
    {
        let cache = GlobalStubCache::new(cache_dir.clone());
        cache.store(&key, &stub);
    }

    // 3. Load from a new cache instance (simulating a restart)
    {
        let cache = GlobalStubCache::new(cache_dir);
        let cached = cache
            .lookup(&key, "com.example.Foo")
            .expect("Should find cached stub");

        // 4. Verify metadata is reconstituted as JavaIndexMetadata
        let meta = cached
            .metadata
            .as_any()
            .downcast_ref::<JavaIndexMetadata>()
            .expect("Metadata should be downcastable to JavaIndexMetadata");

        if let JavaIndexMetadata::Class { modifiers } = meta {
            assert!(modifiers.contains(&"public".to_string()));
            assert!(modifiers.contains(&"final".to_string()));
        } else {
            panic!("Metadata should be of Class type");
        }
    }
}
