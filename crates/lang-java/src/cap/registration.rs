use crate::JavaPlugin;
use naviscope_api::models::Language;
use naviscope_plugin::{LanguageCaps, SemanticCap};
use std::sync::Arc;

pub fn java_caps() -> std::result::Result<LanguageCaps, Box<dyn std::error::Error + Send + Sync>> {
    let plugin = Arc::new(JavaPlugin::new()?);
    Ok(LanguageCaps {
        language: Language::JAVA,
        matcher: plugin.clone(),
        parser: plugin.clone(),
        semantic: plugin.clone() as Arc<dyn SemanticCap>,
        indexing: plugin.clone(),
        asset: plugin.clone(),
        presentation: plugin.clone(),
        metadata_codec: plugin,
    })
}
