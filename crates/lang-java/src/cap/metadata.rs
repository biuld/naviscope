use crate::JavaPlugin;
use naviscope_api::models::graph::EmptyMetadata;
use naviscope_plugin::{CodecContext, MetadataCodecCap, NodeMetadataCodec};
use std::sync::Arc;

impl NodeMetadataCodec for JavaPlugin {
    fn encode_metadata(
        &self,
        metadata: &dyn naviscope_api::models::graph::NodeMetadata,
        _ctx: &mut dyn CodecContext,
    ) -> Vec<u8> {
        if let Some(java_meta) = metadata
            .as_any()
            .downcast_ref::<crate::model::JavaNodeMetadata>()
        {
            rmp_serde::to_vec(&java_meta).unwrap_or_default()
        } else if let Some(java_idx_meta) = metadata
            .as_any()
            .downcast_ref::<crate::model::JavaIndexMetadata>()
        {
            rmp_serde::to_vec(&java_idx_meta).unwrap_or_default()
        } else {
            Vec::new()
        }
    }

    fn decode_metadata(
        &self,
        bytes: &[u8],
        _ctx: &dyn CodecContext,
    ) -> Arc<dyn naviscope_api::models::graph::NodeMetadata> {
        if let Ok(element) = rmp_serde::from_slice::<crate::model::JavaNodeMetadata>(bytes) {
            Arc::new(element)
        } else {
            Arc::new(EmptyMetadata)
        }
    }
}

impl MetadataCodecCap for JavaPlugin {
    fn metadata_codec(&self) -> Option<Arc<dyn NodeMetadataCodec>> {
        Some(Arc::new(self.clone()))
    }
}
