use crate::GradlePlugin;
use naviscope_plugin::{CodecContext, MetadataCodecCap, NodeMetadataCodec};
use std::sync::Arc;

impl NodeMetadataCodec for GradlePlugin {
    fn encode_metadata(
        &self,
        metadata: &dyn naviscope_api::models::graph::NodeMetadata,
        _ctx: &mut dyn CodecContext,
    ) -> Vec<u8> {
        if let Some(gradle_meta) = metadata
            .as_any()
            .downcast_ref::<crate::model::GradleNodeMetadata>()
        {
            rmp_serde::to_vec(&gradle_meta).unwrap_or_default()
        } else {
            Vec::new()
        }
    }

    fn decode_metadata(
        &self,
        bytes: &[u8],
        _ctx: &dyn CodecContext,
    ) -> Arc<dyn naviscope_api::models::graph::NodeMetadata> {
        if let Ok(element) = rmp_serde::from_slice::<crate::model::GradleNodeMetadata>(bytes) {
            Arc::new(element)
        } else {
            Arc::new(naviscope_api::models::graph::EmptyMetadata)
        }
    }
}

impl MetadataCodecCap for GradlePlugin {
    fn metadata_codec(&self) -> Option<Arc<dyn NodeMetadataCodec>> {
        Some(Arc::new(Self::new()))
    }
}
