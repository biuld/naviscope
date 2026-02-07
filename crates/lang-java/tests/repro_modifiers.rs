use naviscope_java::jdk;
use naviscope_java::model::{JavaIndexMetadata, JavaNodeMetadata};
use naviscope_plugin::{IndexMetadata, SymbolInterner};
use std::collections::HashMap;

// Mock context for interning
struct MockContext {
    map: HashMap<String, u32>,
    reverse: HashMap<u32, String>,
    counter: u32,
}

impl MockContext {
    fn new() -> Self {
        Self {
            map: HashMap::new(),
            reverse: HashMap::new(),
            counter: 1,
        }
    }

    fn resolve(&self, id: u32) -> Option<&str> {
        self.reverse.get(&id).map(|s| s.as_str())
    }
}

impl SymbolInterner for MockContext {
    fn intern_str(&mut self, s: &str) -> u32 {
        if let Some(&id) = self.map.get(s) {
            id
        } else {
            let id = self.counter;
            self.counter += 1;
            self.map.insert(s.to_string(), id);
            self.reverse.insert(id, s.to_string());
            id
        }
    }
}

#[test]
fn test_jdk_discovery() {
    let jdk_path = jdk::find_jdk_asset();
    if let Some(path) = jdk_path {
        println!("JDK discovered at: {:?}", path);
        assert!(path.exists(), "Discovered JDK path must exist");

        // Check if it's a valid asset (jimage or jar)
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        assert!(
            ext == "jar" || path.ends_with("modules"),
            "JDK asset must be a JAR or modules file"
        );
    } else {
        println!("WARNING: No JDK found in environment. This is expected if JAVA_HOME is not set.");
    }
}

#[test]
fn test_metadata_serialization_roundtrip() {
    let modifiers = vec![
        "public".to_string(),
        "final".to_string(),
        "abstract".to_string(),
    ];
    let metadata = JavaIndexMetadata::Class {
        modifiers: modifiers.clone(),
    };

    // Serialize JavaIndexMetadata (simulating cache storage)
    let cached = metadata.to_cached_metadata();
    assert_eq!(cached.version, 1);
    assert!(!cached.data.is_empty());

    // Deserialize back using helper
    // Note: In real app, we register this deserializer. Here we call it directly.
    let deserialized = JavaIndexMetadata::deserialize_for_cache(cached.version, &cached.data);

    // Convert to Any to downcast/check
    let back = deserialized
        .as_any()
        .downcast_ref::<JavaIndexMetadata>()
        .expect("Should be JavaIndexMetadata");

    match back {
        JavaIndexMetadata::Class { modifiers: m } => {
            assert_eq!(m, &modifiers);
        }
        _ => panic!("Wrong variant"),
    }
}

#[test]
fn test_node_metadata_interning() {
    let mut ctx = MockContext::new();
    let modifiers = vec!["public".to_string(), "static".to_string()];
    let metadata = JavaIndexMetadata::Class {
        modifiers: modifiers.clone(),
    };

    let interned = metadata.intern(&mut ctx);

    // Check if it's JavaNodeMetadata
    let node_meta = interned
        .as_any()
        .downcast_ref::<JavaNodeMetadata>()
        .expect("Should be JavaNodeMetadata");

    match node_meta {
        JavaNodeMetadata::Class { modifiers_sids } => {
            assert_eq!(modifiers_sids.len(), 2);
            // Verify strings
            let s1 = ctx.resolve(modifiers_sids[0]).unwrap();
            let s2 = ctx.resolve(modifiers_sids[1]).unwrap();

            // Order is preserved
            assert_eq!(s1, "public");
            assert_eq!(s2, "static");
        }
        _ => panic!("Wrong variant"),
    }
}
