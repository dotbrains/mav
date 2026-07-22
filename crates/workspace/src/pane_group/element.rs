use std::mem;
use std::{cell::RefCell, rc::Rc, sync::Arc};

use gpui::{
    Along, AnyElement, App, Axis, BorderStyle, Bounds, CursorStyle, Element, GlobalElementId,
    Hitbox, HitboxBehavior, IntoElement, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    ParentElement, Pixels, Point, Size, Style, WeakEntity, Window, px, relative, size,
};
use parking_lot::Mutex;
use settings::Settings;
use smallvec::SmallVec;
use ui::prelude::*;
use util::ResultExt;

use crate::{Workspace, WorkspaceSettings, workspace_card_gap};

use super::sizing::resize_adjacent_visible_pair;
use super::{HANDLE_HITBOX_SIZE, HORIZONTAL_MIN_SIZE, VERTICAL_MIN_SIZE};

pub(super) fn pane_axis(
    axis: Axis,
    basis: usize,
    flexes: Arc<Mutex<Vec<f32>>>,
    bounding_boxes: Arc<Mutex<Vec<Option<Bounds<Pixels>>>>>,
    workspace: WeakEntity<Workspace>,
) -> PaneAxisElement {
    PaneAxisElement {
        axis,
        basis,
        flexes,
        bounding_boxes,
        children: SmallVec::new(),
        active_pane_ix: None,
        workspace,
        is_leaf_pane_mask: Vec::new(),
        visible_indices: Vec::new(),
    }
}

pub struct PaneAxisElement {
    axis: Axis,
    basis: usize,
    /// Equivalent to ColumnWidths (but in terms of flexes instead of percentages)
    /// For example, flexes "1.33, 1, 1", instead of "40%, 30%, 30%"
    flexes: Arc<Mutex<Vec<f32>>>,
    bounding_boxes: Arc<Mutex<Vec<Option<Bounds<Pixels>>>>>,
    children: SmallVec<[AnyElement; 2]>,
    active_pane_ix: Option<usize>,
    workspace: WeakEntity<Workspace>,
    // Track which children are leaf panes (Member::Pane) vs axes (Member::Axis)
    is_leaf_pane_mask: Vec<bool>,
    visible_indices: Vec<usize>,
}

pub struct PaneAxisLayout {
    dragged_handle: Rc<RefCell<Option<usize>>>,
    children: Vec<PaneAxisChildLayout>,
    visible_indices: Vec<usize>,
}

struct PaneAxisChildLayout {
    bounds: Bounds<Pixels>,
    element: AnyElement,
    handle: Option<PaneAxisHandleLayout>,
    is_leaf_pane: bool,
    original_ix: usize,
}

struct PaneAxisHandleLayout {
    hitbox: Hitbox,
    current_ix: usize,
    next_ix: usize,
}

impl PaneAxisElement {
    pub fn with_active_pane(mut self, active_pane_ix: Option<usize>) -> Self {
        self.active_pane_ix = active_pane_ix;
        self
    }

    pub fn with_is_leaf_pane_mask(mut self, mask: Vec<bool>) -> Self {
        self.is_leaf_pane_mask = mask;
        self
    }

    pub fn with_visible_indices(mut self, indices: Vec<usize>) -> Self {
        self.visible_indices = indices;
        self
    }

    fn compute_resize(
        flexes: &Arc<Mutex<Vec<f32>>>,
        e: &MouseMoveEvent,
        current_ix: usize,
        next_ix: usize,
        visible_indices: &[usize],
        axis: Axis,
        child_start: Point<Pixels>,
        container_size: Size<Pixels>,
        workspace: WeakEntity<Workspace>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let min_size = match axis {
            Axis::Horizontal => px(HORIZONTAL_MIN_SIZE),
            Axis::Vertical => px(VERTICAL_MIN_SIZE),
        };
        let mut flexes = flexes.lock();
        debug_assert!(flex_values_in_bounds(flexes.as_slice()));
        let visible_total_flex = visible_indices.iter().map(|ix| flexes[*ix]).sum::<f32>();
        if visible_total_flex <= 0. {
            return;
        }

        let available_size = Pixels::max(container_size.along(axis), px(0.001));
        let current_size = available_size * (flexes[current_ix] / visible_total_flex);

        let proposed_current_pixel_change = (e.position - child_start).along(axis) - current_size;
        if !resize_adjacent_visible_pair(
            flexes.as_mut_slice(),
            visible_indices,
            current_ix,
            next_ix,
            proposed_current_pixel_change,
            available_size,
            min_size,
        ) {
            return;
        }

        workspace
            .update(cx, |this, cx| this.serialize_workspace(window, cx))
            .log_err();
        cx.stop_propagation();
        window.refresh();
    }

    fn pixel_snap_bounds(window: &Window, bounds: Bounds<Pixels>) -> Bounds<Pixels> {
        let origin = Point::new(
            window.pixel_snap(bounds.left()),
            window.pixel_snap(bounds.top()),
        );
        let corner = Point::new(
            window.pixel_snap(bounds.right()).max(origin.x),
            window.pixel_snap(bounds.bottom()).max(origin.y),
        );
        Bounds::from_corners(origin, corner)
    }

    fn layout_handle(
        axis: Axis,
        pane_bounds: Bounds<Pixels>,
        current_ix: usize,
        next_ix: usize,
        window: &mut Window,
        cx: &mut App,
    ) -> PaneAxisHandleLayout {
        let card_gap = workspace_card_gap(cx);
        let handle_bounds = Bounds {
            origin: pane_bounds.origin.apply_along(axis, |origin| {
                origin + pane_bounds.size.along(axis) - px(HANDLE_HITBOX_SIZE / 2.)
            }),
            size: pane_bounds
                .size
                .apply_along(axis, |_| px(HANDLE_HITBOX_SIZE) + card_gap),
        };

        PaneAxisHandleLayout {
            hitbox: window.insert_hitbox(handle_bounds, HitboxBehavior::BlockMouse),
            current_ix,
            next_ix,
        }
    }
}

impl IntoElement for PaneAxisElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for PaneAxisElement {
    type RequestLayoutState = ();
    type PrepaintState = PaneAxisLayout;

    fn id(&self) -> Option<ElementId> {
        Some(self.basis.into())
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _global_id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (gpui::LayoutId, Self::RequestLayoutState) {
        let style = Style {
            flex_grow: 1.,
            flex_shrink: 1.,
            flex_basis: relative(0.).into(),
            size: size(relative(1.).into(), relative(1.).into()),
            ..Style::default()
        };
        (window.request_layout(style, None, cx), ())
    }

    fn prepaint(
        &mut self,
        global_id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _state: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> PaneAxisLayout {
        let dragged_handle = window.with_element_state::<Rc<RefCell<Option<usize>>>, _>(
            global_id.unwrap(),
            |state, _cx| {
                let state = state.unwrap_or_else(|| Rc::new(RefCell::new(None)));
                (state.clone(), state)
            },
        );
        let flexes = self.flexes.lock().clone();
        let visible_indices = if self.visible_indices.is_empty() {
            (0..self.children.len()).collect::<Vec<_>>()
        } else {
            self.visible_indices.clone()
        };
        let len = visible_indices.len();
        debug_assert!(len == self.children.len());
        debug_assert!(visible_indices.iter().all(|ix| *ix < flexes.len()));
        debug_assert!(flex_values_in_bounds(flexes.as_slice()));

        let total_flex = visible_indices
            .iter()
            .map(|ix| flexes[*ix])
            .sum::<f32>()
            .max(0.001);

        let mut origin = bounds.origin;
        let card_gap = workspace_card_gap(cx);
        let gap_count = len.saturating_sub(1);
        let total_gap = card_gap * gap_count as f32;
        let available_size = Pixels::max(bounds.size.along(self.axis) - total_gap, px(0.0));
        let space_per_flex = available_size / total_flex;

        let mut bounding_boxes = self.bounding_boxes.lock();
        bounding_boxes.clear();
        bounding_boxes.resize(flexes.len(), None);

        let mut layout = PaneAxisLayout {
            dragged_handle,
            children: Vec::new(),
            visible_indices,
        };
        for (ix, mut child) in mem::take(&mut self.children).into_iter().enumerate() {
            let original_ix = layout.visible_indices[ix];
            let child_flex = flexes[original_ix];

            let raw_child_size = bounds
                .size
                .apply_along(self.axis, |_| space_per_flex * child_flex);
            let raw_child_bounds = Bounds {
                origin,
                size: raw_child_size,
            };
            // Pane axes bypass Taffy, so snap their child edges explicitly to avoid
            // 1dp border jitter when flex sizes or card gaps are fractional.
            let child_bounds = Self::pixel_snap_bounds(window, raw_child_bounds);

            bounding_boxes[original_ix] = Some(child_bounds);
            child.layout_as_root(child_bounds.size.into(), window, cx);
            child.prepaint_at(child_bounds.origin, window, cx);

            origin = origin.apply_along(self.axis, |val| {
                val + raw_child_size.along(self.axis) + card_gap
            });

            let is_leaf_pane = self.is_leaf_pane_mask.get(ix).copied().unwrap_or(true);

            layout.children.push(PaneAxisChildLayout {
                bounds: child_bounds,
                element: child,
                handle: None,
                is_leaf_pane,
                original_ix,
            })
        }

        for ix in 0..layout.children.len() {
            if ix < len - 1 {
                let next_ix = layout.children[ix + 1].original_ix;
                let child_layout = &mut layout.children[ix];
                child_layout.handle = Some(Self::layout_handle(
                    self.axis,
                    child_layout.bounds,
                    child_layout.original_ix,
                    next_ix,
                    window,
                    cx,
                ));
            }
        }

        layout
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: gpui::Bounds<ui::prelude::Pixels>,
        _: &mut Self::RequestLayoutState,
        layout: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        for child in &mut layout.children {
            child.element.paint(window, cx);
        }

        let overlay_opacity = WorkspaceSettings::get(None, cx)
            .active_pane_modifiers
            .inactive_opacity
            .map(|val| val.0.clamp(0.0, 1.0))
            .and_then(|val| (val <= 1.).then_some(val));

        let mut overlay_background = cx.theme().colors().editor_background;
        if let Some(opacity) = overlay_opacity {
            overlay_background.fade_out(opacity);
        }

        let overlay_border = WorkspaceSettings::get(None, cx)
            .active_pane_modifiers
            .border_size
            .and_then(|val| (val >= 0.).then_some(val));

        let card_gap = workspace_card_gap(cx);

        for (ix, child) in &mut layout.children.iter_mut().enumerate() {
            if overlay_opacity.is_some() || overlay_border.is_some() {
                // Keep active/inactive overlays inside the pane card border.
                let overlay_bounds = Bounds {
                    origin: Point::new(
                        child.bounds.origin.x + px(1.),
                        child.bounds.origin.y + px(1.),
                    ),
                    size: Size {
                        width: Pixels::max(child.bounds.size.width - px(2.), px(0.)),
                        height: Pixels::max(child.bounds.size.height - px(2.), px(0.)),
                    },
                };

                if overlay_opacity.is_some()
                    && child.is_leaf_pane
                    && self.active_pane_ix != Some(ix)
                {
                    window.paint_quad(gpui::fill(overlay_bounds, overlay_background));
                }

                if let Some(border) = overlay_border
                    && self.active_pane_ix == Some(ix)
                    && child.is_leaf_pane
                {
                    window.paint_quad(gpui::quad(
                        overlay_bounds,
                        0.,
                        gpui::transparent_black(),
                        border,
                        cx.theme().colors().border_selected,
                        BorderStyle::Solid,
                    ));
                }
            }

            if let Some(handle) = child.handle.as_mut() {
                let cursor_style = match self.axis {
                    Axis::Vertical => CursorStyle::ResizeRow,
                    Axis::Horizontal => CursorStyle::ResizeColumn,
                };

                if layout
                    .dragged_handle
                    .borrow()
                    .is_some_and(|dragged_ix| dragged_ix == ix)
                {
                    window.set_window_cursor_style(cursor_style);
                } else {
                    window.set_cursor_style(cursor_style, &handle.hitbox);
                }

                window.on_mouse_event({
                    let dragged_handle = layout.dragged_handle.clone();
                    let flexes = self.flexes.clone();
                    let workspace = self.workspace.clone();
                    let handle_hitbox = handle.hitbox.clone();
                    move |e: &MouseDownEvent, phase, window, cx| {
                        if phase.bubble() && handle_hitbox.is_hovered(window) {
                            dragged_handle.replace(Some(ix));
                            if e.click_count >= 2 {
                                let mut borrow = flexes.lock();
                                *borrow = vec![1.; borrow.len()];
                                workspace
                                    .update(cx, |this, cx| this.serialize_workspace(window, cx))
                                    .log_err();

                                window.refresh();
                            }
                            cx.stop_propagation();
                        }
                    }
                });
                window.on_mouse_event({
                    let workspace = self.workspace.clone();
                    let dragged_handle = layout.dragged_handle.clone();
                    let flexes = self.flexes.clone();
                    let child_bounds = child.bounds;
                    let axis = self.axis;
                    let current_ix = handle.current_ix;
                    let next_ix = handle.next_ix;
                    let visible_indices = layout.visible_indices.clone();
                    move |e: &MouseMoveEvent, phase, window, cx| {
                        let dragged_handle = dragged_handle.borrow();
                        if phase.bubble() && *dragged_handle == Some(ix) {
                            Self::compute_resize(
                                &flexes,
                                e,
                                current_ix,
                                next_ix,
                                &visible_indices,
                                axis,
                                child_bounds.origin,
                                bounds.size.apply_along(axis, |size| {
                                    Pixels::max(
                                        size - card_gap
                                            * visible_indices.len().saturating_sub(1) as f32,
                                        px(0.0),
                                    )
                                }),
                                workspace.clone(),
                                window,
                                cx,
                            )
                        }
                    }
                });
            }
        }

        window.on_mouse_event({
            let dragged_handle = layout.dragged_handle.clone();
            move |_: &MouseUpEvent, phase, _window, _cx| {
                if phase.bubble() {
                    dragged_handle.replace(None);
                }
            }
        });
    }
}

impl ParentElement for PaneAxisElement {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements)
    }
}

fn flex_values_in_bounds(flexes: &[f32]) -> bool {
    (flexes.iter().copied().sum::<f32>() - flexes.len() as f32).abs() < 0.001
}
