use super::*;

#[derive(Default)]
pub(crate) struct ScrollbarMarkerState {
    pub(crate) scrollbar_size: Size<Pixels>,
    pub(crate) dirty: bool,
    pub(crate) markers: Arc<[PaintQuad]>,
    pub(crate) pending_refresh: Option<Task<Result<()>>>,
}

impl ScrollbarMarkerState {
    pub(crate) fn should_refresh(&self, scrollbar_size: Size<Pixels>) -> bool {
        self.pending_refresh.is_none() && (self.scrollbar_size != scrollbar_size || self.dirty)
    }
}
