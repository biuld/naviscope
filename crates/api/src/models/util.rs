use serde::{Deserialize, Deserializer, Serializer};
use std::path::Path;
use std::sync::Arc;

pub mod serde_arc_str {
    use super::*;

    pub fn serialize<S>(arc: &Arc<str>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(arc)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Arc<str>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(Arc::from(s.as_str()))
    }
}

pub mod serde_arc_path {
    use super::*;

    pub fn serialize<S>(arc: &Arc<Path>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&arc.to_string_lossy())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Arc<Path>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(Arc::from(Path::new(&s)))
    }
}
