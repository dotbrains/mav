use std::marker::PhantomData;

use gpui::{Context, Point, Render, Window};
use ui::prelude::*;

use crate::{
    preview::Layout,
    shape::{PositionAndShape, Shape, SizeBounds},
};

pub(crate) struct DragPreview;

impl Render for DragPreview {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

#[derive(Clone, Copy)]
pub(crate) struct ResizeDrag<S> {
    pub(crate) shape_before: PositionAndShape,
    pub(crate) phantom_data: PhantomData<S>,
    pub(crate) mouse_pos_before: Point<Pixels>,
}

impl<S> ResizeDrag<S> {
    pub(crate) fn start_new(
        shape: Shape,
        bounds: &SizeBounds,
        layout: Option<Layout>,
        window: &mut Window,
    ) -> Self {
        Self {
            mouse_pos_before: window.mouse_position(),
            // Before rendering we always clamp so the current shape may not be
            // within SizeBounds, so use a clamped one.
            shape_before: shape.clamped_position_and_size(layout, bounds, window),
            phantom_data: PhantomData,
        }
    }
}
