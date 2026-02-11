use crate::codec::CodecContext;
use naviscope_api::models::graph::{NodeMetadata};
use std::sync::Arc;

pub trait NodeMetadataCodec: Send + Sync {
    fn encode_metadata(&self, metadata: &dyn NodeMetadata, ctx: &mut dyn CodecContext) -> Vec<u8>;
    fn decode_metadata(&self, bytes: &[u8], ctx: &dyn CodecContext) -> Arc<dyn NodeMetadata>;
}
