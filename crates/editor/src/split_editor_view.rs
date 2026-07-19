use std::cmp;

use collections::{HashMap, HashSet};
use gpui::{
    AbsoluteLength, AnyElement, App, AvailableSpace, Bounds, Context, DragMoveEvent, Element,
    Entity, GlobalElementId, Hsla, InspectorElementId, IntoElement, LayoutId, Length,
    ParentElement, Pixels, StatefulInteractiveElement, Styled, TextStyleRefinement, Window, div,
    linear_color_stop, linear_gradient, point, px, size,
};
use multi_buffer::{Anchor, ExcerptBoundaryInfo};
use smallvec::smallvec;
use text::BufferId;
use theme::ActiveTheme;
use ui::{h_flex, prelude::*, v_flex};

use gpui::ContentMask;

use crate::{
    DisplayRow, Editor, EditorSnapshot, EditorStyle, FILE_HEADER_HEIGHT,
    MULTI_BUFFER_EXCERPT_HEADER_HEIGHT, RowExt, StickyHeaderExcerpt,
    display_map::Block,
    element::{EditorElement, SplitSide, header_jump_data, render_buffer_header},
    scroll::ScrollOffset,
    split::SplittableEditor,
};

const RESIZE_HANDLE_WIDTH: f32 = 8.0;

#[derive(Debug, Clone)]
struct DraggedSplitHandle;

pub struct SplitEditorState {
    left_ratio: f32,
    visible_left_ratio: f32,
    cached_width: Pixels,
}

impl SplitEditorState {
    pub fn new(_cx: &mut App) -> Self {
        Self {
            left_ratio: 0.5,
            visible_left_ratio: 0.5,
            cached_width: px(0.),
        }
    }

    #[allow(clippy::misnamed_getters)]
    pub fn left_ratio(&self) -> f32 {
        self.visible_left_ratio
    }

    pub fn right_ratio(&self) -> f32 {
        1.0 - self.visible_left_ratio
    }

    fn on_drag_move(
        &mut self,
        drag_event: &DragMoveEvent<DraggedSplitHandle>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        let drag_position = drag_event.event.position;
        let bounds = drag_event.bounds;
        let bounds_width = bounds.right() - bounds.left();

        if bounds_width > px(0.) {
            self.cached_width = bounds_width;
        }

        let min_ratio = 0.1;
        let max_ratio = 0.9;

        let new_ratio = (drag_position.x - bounds.left()) / bounds_width;
        self.visible_left_ratio = new_ratio.clamp(min_ratio, max_ratio);
    }

    fn commit_ratio(&mut self) {
        self.left_ratio = self.visible_left_ratio;
    }

    fn on_double_click(&mut self) {
        self.left_ratio = 0.5;
        self.visible_left_ratio = 0.5;
    }
}

#[derive(IntoElement)]
pub struct SplitEditorView {
    splittable_editor: Entity<SplittableEditor>,
    style: EditorStyle,
    split_state: Entity<SplitEditorState>,
}

impl SplitEditorView {
    pub fn new(
        splittable_editor: Entity<SplittableEditor>,
        style: EditorStyle,
        split_state: Entity<SplitEditorState>,
    ) -> Self {
        Self {
            splittable_editor,
            style,
            split_state,
        }
    }
}

fn render_resize_handle(
    state: &Entity<SplitEditorState>,
    separator_color: Hsla,
    _window: &mut Window,
    _cx: &mut App,
) -> AnyElement {
    let state_for_click = state.clone();

    div()
        .id("split-resize-container")
        .relative()
        .h_full()
        .flex_shrink_0()
        .w(px(1.))
        .bg(separator_color)
        .child(
            div()
                .id("split-resize-handle")
                .absolute()
                .left(px(-RESIZE_HANDLE_WIDTH / 2.0))
                .w(px(RESIZE_HANDLE_WIDTH))
                .h_full()
                .cursor_col_resize()
                .block_mouse_except_scroll()
                .on_click(move |event, _, cx| {
                    if event.click_count() >= 2 {
                        state_for_click.update(cx, |state, _| {
                            state.on_double_click();
                        });
                    }
                    cx.stop_propagation();
                })
                .on_drag(DraggedSplitHandle, |_, _, _, cx| cx.new(|_| gpui::Empty)),
        )
        .into_any_element()
}

impl RenderOnce for SplitEditorView {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let splittable_editor = self.splittable_editor.read(cx);

        assert!(
            splittable_editor.lhs_editor().is_some(),
            "`SplitEditorView` requires `SplittableEditor` to be in split mode"
        );

        let lhs_editor = splittable_editor.lhs_editor().unwrap().clone();
        let rhs_editor = splittable_editor.rhs_editor().clone();

        let mut lhs = EditorElement::new(&lhs_editor, self.style.clone());
        let mut rhs = EditorElement::new(&rhs_editor, self.style.clone());

        lhs.set_split_side(SplitSide::Left);
        rhs.set_split_side(SplitSide::Right);

        let left_ratio = self.split_state.read(cx).left_ratio();
        let right_ratio = self.split_state.read(cx).right_ratio();

        let separator_color = cx.theme().colors().border_variant;

        let resize_handle = render_resize_handle(&self.split_state, separator_color, window, cx);

        let state_for_drag = self.split_state.downgrade();
        let state_for_drop = self.split_state.downgrade();

        let buffer_headers = SplitBufferHeadersElement::new(
            lhs_editor.clone(),
            rhs_editor.clone(),
            self.style.clone(),
        );

        let lhs_editor_for_order = lhs_editor;
        let rhs_editor_for_order = rhs_editor;

        div()
            .id("split-editor-view-container")
            .size_full()
            .relative()
            .child(
                h_flex()
                    .with_dynamic_prepaint_order(move |_window, cx| {
                        let lhs_needs = lhs_editor_for_order.read(cx).has_autoscroll_request();
                        let rhs_needs = rhs_editor_for_order.read(cx).has_autoscroll_request();
                        match (lhs_needs, rhs_needs) {
                            (false, true) => smallvec![2, 1, 0],
                            _ => smallvec![0, 1, 2],
                        }
                    })
                    .id("split-editor-view")
                    .size_full()
                    .on_drag_move::<DraggedSplitHandle>(move |event, window, cx| {
                        state_for_drag
                            .update(cx, |state, cx| {
                                state.on_drag_move(event, window, cx);
                            })
                            .ok();
                    })
                    .on_drop::<DraggedSplitHandle>(move |_, _, cx| {
                        state_for_drop
                            .update(cx, |state, _| {
                                state.commit_ratio();
                            })
                            .ok();
                    })
                    .child(
                        div()
                            .id("split-editor-left")
                            .flex_shrink_1()
                            .min_w_0()
                            .h_full()
                            .flex_basis(DefiniteLength::Fraction(left_ratio))
                            .overflow_hidden()
                            .child(lhs),
                    )
                    .child(resize_handle)
                    .child(
                        div()
                            .id("split-editor-right")
                            .flex_shrink_1()
                            .min_w_0()
                            .h_full()
                            .flex_basis(DefiniteLength::Fraction(right_ratio))
                            .overflow_hidden()
                            .child(rhs),
                    ),
            )
            .child(buffer_headers)
    }
}

mod buffer_headers;
use buffer_headers::SplitBufferHeadersElement;
