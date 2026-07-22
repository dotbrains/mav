use crate::{
    AnyActiveCall, AppState, CollaboratorId, FollowerState, Pane, ParticipantLocation, Workspace,
    WorkspaceSettings,
    notifications::DetachAndPromptErr,
    workspace_card_gap,
    workspace_settings::{PaneSplitDirectionHorizontal, PaneSplitDirectionVertical},
};
use anyhow::Result;
use collections::HashMap;
use gpui::{
    Along, AnyView, AnyWeakView, Axis, Bounds, Entity, Hsla, IntoElement, MouseButton, Pixels,
    Point, StyleRefinement, WeakEntity, Window, point, size,
};
use parking_lot::Mutex;
use project::Project;
use schemars::JsonSchema;
use serde::Deserialize;
use settings::Settings;
use std::{cmp::Ordering, mem, sync::Arc};
use ui::prelude::*;

mod axis;
mod axis_resize_render;
mod element;
mod member;
mod sizing;
mod split_direction;

pub use axis::PaneAxis;
use element::pane_axis;
pub use member::{
    ActivePaneDecorator, LeaderDecoration, Member, PaneLeaderDecorator, PaneRenderContext,
};
pub use split_direction::SplitDirection;

pub const HANDLE_HITBOX_SIZE: f32 = 4.0;
const HORIZONTAL_MIN_SIZE: f32 = 80.;
const VERTICAL_MIN_SIZE: f32 = 100.;

#[derive(Clone, Copy, Debug)]
pub struct SplitSizeHint {
    inserted_size: Pixels,
    available_size: Option<Pixels>,
}

impl SplitSizeHint {
    pub fn inserted_size(inserted_size: Pixels) -> Self {
        Self {
            inserted_size,
            available_size: None,
        }
    }

    pub fn inserted_size_in_available_space(inserted_size: Pixels, available_size: Pixels) -> Self {
        Self {
            inserted_size,
            available_size: Some(available_size),
        }
    }
}

/// One or many panes, arranged in a horizontal or vertical axis due to a split.
/// Panes have all their tabs and capabilities preserved, and can be split again or resized.
/// Single-pane group is a regular pane.
#[derive(Clone)]
pub struct PaneGroup {
    pub root: Member,
    pub is_center: bool,
}

pub struct PaneRenderResult {
    pub element: gpui::AnyElement,
    pub contains_active_pane: bool,
}

impl PaneGroup {
    pub fn with_root(mut root: Member) -> Self {
        root.normalize_same_axis();
        Self {
            root,
            is_center: false,
        }
    }

    pub fn new(pane: Entity<Pane>) -> Self {
        Self {
            root: Member::Pane(pane),
            is_center: false,
        }
    }

    pub fn set_is_center(&mut self, is_center: bool) {
        self.is_center = is_center;
    }

    pub fn split(
        &mut self,
        old_pane: &Entity<Pane>,
        new_pane: &Entity<Pane>,
        direction: SplitDirection,
        cx: &mut App,
    ) {
        self.split_with_size_hint(old_pane, new_pane, direction, None, cx);
    }

    pub fn split_with_size_hint(
        &mut self,
        old_pane: &Entity<Pane>,
        new_pane: &Entity<Pane>,
        direction: SplitDirection,
        size_hint: Option<SplitSizeHint>,
        cx: &mut App,
    ) {
        let found = match &mut self.root {
            Member::Pane(pane) => {
                if pane == old_pane {
                    self.root = Member::new_axis_with_size_hint(
                        old_pane.clone(),
                        new_pane.clone(),
                        direction,
                        size_hint,
                        None,
                    );
                    true
                } else {
                    false
                }
            }
            Member::Axis(axis) => axis.split(old_pane, new_pane, direction, size_hint, cx),
        };

        // If the pane wasn't found, fall back to splitting the first pane in the tree.
        if !found {
            let first_pane = self.root.first_pane();
            match &mut self.root {
                Member::Pane(_) => {
                    self.root = Member::new_axis_with_size_hint(
                        first_pane,
                        new_pane.clone(),
                        direction,
                        size_hint,
                        None,
                    );
                }
                Member::Axis(axis) => {
                    let _ = axis.split(&first_pane, new_pane, direction, size_hint, cx);
                }
            }
        }

        self.mark_positions(cx);
    }

    pub fn bounding_box_for_pane(&self, pane: &Entity<Pane>) -> Option<Bounds<Pixels>> {
        match &self.root {
            Member::Pane(_) => None,
            Member::Axis(axis) => axis.bounding_box_for_pane(pane),
        }
    }

    pub fn horizontal_size_for_pane(&self, pane: &Entity<Pane>) -> Option<Pixels> {
        match &self.root {
            Member::Pane(_) => None,
            Member::Axis(axis) => axis.horizontal_size_for_pane(pane),
        }
    }

    pub fn full_height_column_count(&self) -> usize {
        self.root.full_height_column_count()
    }

    pub fn pane_at_pixel_position(&self, coordinate: Point<Pixels>) -> Option<&Entity<Pane>> {
        match &self.root {
            Member::Pane(pane) => Some(pane),
            Member::Axis(axis) => axis.pane_at_pixel_position(coordinate),
        }
    }

    /// Moves active pane to span the entire border in the given direction,
    /// similar to Vim ctrl+w shift-[hjkl] motion.
    ///
    /// Returns:
    /// - Ok(true) if it found and moved a pane
    /// - Ok(false) if it found but did not move the pane
    /// - Err(_) if it did not find the pane
    pub fn move_to_border(
        &mut self,
        active_pane: &Entity<Pane>,
        direction: SplitDirection,
        cx: &mut App,
    ) -> Result<bool> {
        if let Some(pane) = self.find_pane_at_border(direction)
            && pane == active_pane
        {
            return Ok(false);
        }

        if !self.remove_internal(active_pane, cx)? {
            return Ok(false);
        }

        if let Member::Axis(root) = &mut self.root
            && direction.axis() == root.axis
        {
            let idx = if direction.increasing() {
                root.members.len()
            } else {
                0
            };
            root.insert_moved_pane(idx, active_pane);
            self.mark_positions(cx);
            return Ok(true);
        }

        let members = if direction.increasing() {
            vec![self.root.clone(), Member::Pane(active_pane.clone())]
        } else {
            vec![Member::Pane(active_pane.clone()), self.root.clone()]
        };
        self.root = Member::Axis(PaneAxis::new(direction.axis(), members));
        self.mark_positions(cx);
        Ok(true)
    }

    fn find_pane_at_border(&self, direction: SplitDirection) -> Option<&Entity<Pane>> {
        match &self.root {
            Member::Pane(pane) => Some(pane),
            Member::Axis(axis) => axis.find_pane_at_border(direction),
        }
    }

    /// Returns:
    /// - Ok(true) if it found and removed a pane
    /// - Ok(false) if it found but did not remove the pane
    /// - Err(_) if it did not find the pane
    pub fn remove(&mut self, pane: &Entity<Pane>, cx: &mut App) -> Result<bool> {
        let result = self.remove_internal(pane, cx);
        if let Ok(true) = result {
            self.mark_positions(cx);
        }
        result
    }

    fn remove_internal(&mut self, pane: &Entity<Pane>, cx: &App) -> Result<bool> {
        match &mut self.root {
            Member::Pane(_) => Ok(false),
            Member::Axis(axis) => {
                if let Some(last_pane) = axis.remove(pane, cx)? {
                    self.root = last_pane;
                }
                Ok(true)
            }
        }
    }

    pub fn resize(
        &mut self,
        pane: &Entity<Pane>,
        direction: Axis,
        amount: Pixels,
        bounds: &Bounds<Pixels>,
        cx: &mut App,
    ) {
        match &mut self.root {
            Member::Pane(_) => {}
            Member::Axis(axis) => {
                let _ = axis.resize(pane, direction, amount, bounds);
            }
        };
        self.mark_positions(cx);
    }

    pub fn reset_pane_sizes(&mut self, cx: &mut App) {
        match &mut self.root {
            Member::Pane(_) => {}
            Member::Axis(axis) => {
                let _ = axis.reset_pane_sizes();
            }
        };
        self.mark_positions(cx);
    }

    pub fn swap(&mut self, from: &Entity<Pane>, to: &Entity<Pane>, cx: &mut App) {
        match &mut self.root {
            Member::Pane(_) => {}
            Member::Axis(axis) => axis.swap(from, to),
        };
        self.mark_positions(cx);
    }

    pub fn mark_positions(&mut self, cx: &mut App) {
        self.root.normalize_same_axis();
        let top_left_pane = self
            .is_center
            .then(|| self.root.first_visible_pane(cx))
            .flatten();
        self.root
            .mark_positions(self.is_center, top_left_pane.as_ref(), cx);
    }

    pub fn render(
        &self,
        zoomed: Option<&AnyWeakView>,
        render_cx: &dyn PaneLeaderDecorator,
        window: &mut Window,
        cx: &mut App,
    ) -> impl IntoElement {
        self.root.render(0, zoomed, render_cx, window, cx).element
    }

    pub fn panes(&self) -> Vec<&Entity<Pane>> {
        let mut panes = Vec::new();
        self.root.collect_panes(&mut panes);
        panes
    }

    pub fn first_pane(&self) -> Entity<Pane> {
        self.root.first_pane()
    }

    pub fn last_pane(&self) -> Entity<Pane> {
        self.root.last_pane()
    }

    pub fn find_pane_in_direction(
        &self,
        active_pane: &Entity<Pane>,
        direction: SplitDirection,
        cx: &App,
    ) -> Option<Entity<Pane>> {
        let bounding_box = self.bounding_box_for_pane(active_pane)?;
        let cursor = active_pane.read(cx).pixel_position_of_cursor(cx);
        let center = match cursor {
            Some(cursor) if bounding_box.contains(&cursor) => cursor,
            _ => bounding_box.center(),
        };

        let mut pane_bounds = Vec::new();
        self.root.collect_pane_bounds(&mut pane_bounds);
        pane_bounds
            .into_iter()
            .filter(|(pane, _)| pane != active_pane)
            .filter_map(|(pane, candidate_bounds)| {
                pane_distances_in_direction(bounding_box, candidate_bounds, center, direction)
                    .map(|distances| (pane, distances))
            })
            .min_by(|(_, left), (_, right)| compare_pane_distances(*left, *right))
            .map(|(pane, _)| pane)
    }

    pub fn invert_axies(&mut self, cx: &mut App) {
        self.root.invert_pane_axies();
        self.mark_positions(cx);
    }
}

pub(crate) fn render_pane_card(
    pane: Entity<Pane>,
    render_cx: &dyn PaneLeaderDecorator,
    cx: &mut App,
) -> AnyElement {
    let decoration = render_cx.decorate(&pane, cx);

    div()
        .relative()
        .flex_1()
        .size_full()
        .overflow_hidden()
        .rounded_lg()
        .border_1()
        .border_color(cx.theme().colors().border)
        .child(AnyView::from(pane).cached(StyleRefinement::default().v_flex().size_full()))
        .when_some(decoration.border, |this, color| {
            this.child(
                div()
                    .absolute()
                    .size_full()
                    .left_0()
                    .top_0()
                    .border_2()
                    .border_color(color),
            )
        })
        .children(decoration.status_box)
        .into_any()
}

type PaneDistances = (f32, f32, f32);

fn pane_distances_in_direction(
    active: Bounds<Pixels>,
    candidate: Bounds<Pixels>,
    anchor: Point<Pixels>,
    direction: SplitDirection,
) -> Option<PaneDistances> {
    let (primary, cross, center) = match direction {
        SplitDirection::Left => {
            let primary = active.left() - candidate.right();
            if primary < Pixels::ZERO {
                return None;
            }
            let cross = distance_to_interval(anchor.y, candidate.top(), candidate.bottom());
            let center = (candidate.center().y - anchor.y).as_f32().abs();
            (primary, cross, center)
        }
        SplitDirection::Right => {
            let primary = candidate.left() - active.right();
            if primary < Pixels::ZERO {
                return None;
            }
            let cross = distance_to_interval(anchor.y, candidate.top(), candidate.bottom());
            let center = (candidate.center().y - anchor.y).as_f32().abs();
            (primary, cross, center)
        }
        SplitDirection::Up => {
            let primary = active.top() - candidate.bottom();
            if primary < Pixels::ZERO {
                return None;
            }
            let cross = distance_to_interval(anchor.x, candidate.left(), candidate.right());
            let center = (candidate.center().x - anchor.x).as_f32().abs();
            (primary, cross, center)
        }
        SplitDirection::Down => {
            let primary = candidate.top() - active.bottom();
            if primary < Pixels::ZERO {
                return None;
            }
            let cross = distance_to_interval(anchor.x, candidate.left(), candidate.right());
            let center = (candidate.center().x - anchor.x).as_f32().abs();
            (primary, cross, center)
        }
    };

    Some((primary.as_f32(), cross.as_f32(), center))
}

fn distance_to_interval(value: Pixels, start: Pixels, end: Pixels) -> Pixels {
    if value < start {
        start - value
    } else if value > end {
        value - end
    } else {
        Pixels::ZERO
    }
}

fn compare_pane_distances(left: PaneDistances, right: PaneDistances) -> Ordering {
    left.0
        .total_cmp(&right.0)
        .then_with(|| left.1.total_cmp(&right.1))
        .then_with(|| left.2.total_cmp(&right.2))
}
