use crate::interner::FqnInterner;

pub trait CodecContext: Send + Sync {
    fn interner(&mut self) -> &mut dyn FqnInterner;
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;
}
