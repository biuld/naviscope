use crate::cap::{
    AssetCap, BuildIndexCap, BuildParseCap, FileMatcherCap, LanguageParseCap, MetadataCodecCap,
    PresentationCap, SemanticCap, SourceIndexCap,
};
use naviscope_api::models::{BuildTool, Language};
use std::sync::Arc;

#[derive(Clone)]
pub struct LanguageCaps {
    pub language: Language,
    pub matcher: Arc<dyn FileMatcherCap>,
    pub parser: Arc<dyn LanguageParseCap>,
    pub semantic: Arc<dyn SemanticCap>,
    pub indexing: Arc<dyn SourceIndexCap>,
    pub asset: Arc<dyn AssetCap>,
    pub presentation: Arc<dyn PresentationCap>,
    pub metadata_codec: Arc<dyn MetadataCodecCap>,
}

#[derive(Clone)]
pub struct BuildCaps {
    pub build_tool: BuildTool,
    pub matcher: Arc<dyn FileMatcherCap>,
    pub parser: Arc<dyn BuildParseCap>,
    pub indexing: Arc<dyn BuildIndexCap>,
    pub asset: Arc<dyn AssetCap>,
    pub presentation: Arc<dyn PresentationCap>,
    pub metadata_codec: Arc<dyn MetadataCodecCap>,
}
