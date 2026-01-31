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

pub fn utf16_col_to_byte_col(content: &str, line: usize, utf16_col: usize) -> usize {
    let line_content = content.lines().nth(line).unwrap_or("");
    let mut curr_utf16 = 0;
    let mut curr_byte = 0;

    for c in line_content.chars() {
        if curr_utf16 >= utf16_col {
            break;
        }
        curr_utf16 += c.len_utf16();
        curr_byte += c.len_utf8();
    }
    curr_byte
}
