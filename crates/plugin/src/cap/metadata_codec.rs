use crate::metadata_codec::NodeMetadataCodec;
use std::sync::Arc;

pub trait MetadataCodecCap: Send + Sync {
    fn metadata_codec(&self) -> Option<Arc<dyn NodeMetadataCodec>> {
        None
    }
}
