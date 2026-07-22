use super::sizing::resize_adjacent_visible_pair;
use super::*;

impl PaneAxis {
    pub(super) fn resize(
        &mut self,
        pane: &Entity<Pane>,
        axis: Axis,
        amount: Pixels,
        bounds: &Bounds<Pixels>,
    ) -> Option<bool> {
        let found_pane = self
            .members
            .iter()
            .any(|member| matches!(member, Member::Pane(p) if p == pane));

        if found_pane && self.axis != axis {
            return Some(false); // pane found but this is not the correct axis direction
        }
        let mut found_axis_index: Option<usize> = None;
        if !found_pane {
            for (i, pa) in self.members.iter_mut().enumerate() {
                if let Member::Axis(pa) = pa
                    && let Some(done) = pa.resize(pane, axis, amount, bounds)
                {
                    if done {
                        return Some(true); // pane found and operations already done
                    } else if self.axis != axis {
                        return Some(false); // pane found but this is not the correct axis direction
                    } else {
                        found_axis_index = Some(i); // pane found and this is correct direction
                    }
                }
            }
            found_axis_index?; // no pane found
        }

        let min_size = match axis {
            Axis::Horizontal => px(HORIZONTAL_MIN_SIZE),
            Axis::Vertical => px(VERTICAL_MIN_SIZE),
        };
        let mut flexes = self.flexes.lock();

        let ix = if found_pane {
            self.members.iter().position(|m| {
                if let Member::Pane(p) = m {
                    p == pane
                } else {
                    false
                }
            })
        } else {
            found_axis_index
        };

        if ix.is_none() {
            return Some(true);
        }

        let ix = ix.unwrap_or(0);

        let (visible_indices, available_size) = {
            let bounding_boxes = self.bounding_boxes.lock();
            let indices = bounding_boxes
                .iter()
                .enumerate()
                .filter_map(|(ix, bounds)| bounds.is_some().then_some(ix))
                .collect::<Vec<_>>();
            if indices.is_empty() {
                ((0..self.members.len()).collect(), bounds.size.along(axis))
            } else {
                let available_size = indices
                    .iter()
                    .filter_map(|ix| bounding_boxes[*ix].map(|bounds| bounds.size.along(axis)))
                    .sum();
                (indices, available_size)
            }
        };
        let Some(visible_ix) = visible_indices
            .iter()
            .position(|visible_member_ix| *visible_member_ix == ix)
        else {
            return Some(true);
        };

        if visible_ix + 1 == visible_indices.len() {
            if visible_ix == 0 {
                return Some(true);
            }
            resize_adjacent_visible_pair(
                flexes.as_mut_slice(),
                &visible_indices,
                visible_indices[visible_ix - 1],
                ix,
                -amount,
                available_size,
                min_size,
            );
        } else {
            resize_adjacent_visible_pair(
                flexes.as_mut_slice(),
                &visible_indices,
                ix,
                visible_indices[visible_ix + 1],
                amount,
                available_size,
                min_size,
            );
        }
        Some(true)
    }

    pub(super) fn swap(&mut self, from: &Entity<Pane>, to: &Entity<Pane>) {
        for member in self.members.iter_mut() {
            match member {
                Member::Axis(axis) => axis.swap(from, to),
                Member::Pane(pane) => {
                    if pane == from {
                        *member = Member::Pane(to.clone());
                    } else if pane == to {
                        *member = Member::Pane(from.clone())
                    }
                }
            }
        }
    }

    pub(super) fn bounding_box_for_pane(&self, pane: &Entity<Pane>) -> Option<Bounds<Pixels>> {
        debug_assert!(self.members.len() == self.bounding_boxes.lock().len());

        for (idx, member) in self.members.iter().enumerate() {
            match member {
                Member::Pane(found) => {
                    if pane == found {
                        return self.bounding_boxes.lock()[idx];
                    }
                }
                Member::Axis(axis) => {
                    if let Some(rect) = axis.bounding_box_for_pane(pane) {
                        return Some(rect);
                    }
                }
            }
        }
        None
    }

    pub(super) fn pane_at_pixel_position(
        &self,
        coordinate: Point<Pixels>,
    ) -> Option<&Entity<Pane>> {
        debug_assert!(self.members.len() == self.bounding_boxes.lock().len());

        let bounding_boxes = self.bounding_boxes.lock();

        for (idx, member) in self.members.iter().enumerate() {
            if let Some(coordinates) = bounding_boxes[idx]
                && coordinates.contains(&coordinate)
            {
                return match member {
                    Member::Pane(found) => Some(found),
                    Member::Axis(axis) => axis.pane_at_pixel_position(coordinate),
                };
            }
        }
        None
    }

    pub(super) fn collect_pane_bounds(&self, panes: &mut Vec<(Entity<Pane>, Bounds<Pixels>)>) {
        debug_assert!(self.members.len() == self.bounding_boxes.lock().len());

        let bounding_boxes = self.bounding_boxes.lock();
        for (idx, member) in self.members.iter().enumerate() {
            let Some(bounds) = bounding_boxes[idx] else {
                continue;
            };

            match member {
                Member::Pane(pane) => panes.push((pane.clone(), bounds)),
                Member::Axis(axis) => axis.collect_pane_bounds(panes),
            }
        }
    }

    pub(super) fn full_height_column_count(&self) -> usize {
        match self.axis {
            Axis::Horizontal => self
                .members
                .iter()
                .map(Member::full_height_column_count)
                .sum::<usize>()
                .max(1),
            Axis::Vertical => self
                .members
                .iter()
                .map(Member::full_height_column_count)
                .max()
                .unwrap_or(1),
        }
    }

    pub(super) fn render(
        &self,
        basis: usize,
        zoomed: Option<&AnyWeakView>,
        render_cx: &dyn PaneLeaderDecorator,
        window: &mut Window,
        cx: &mut App,
    ) -> PaneRenderResult {
        debug_assert!(self.members.len() == self.flexes.lock().len());
        let mut active_pane_ix = None;
        let mut contains_active_pane = false;
        let mut is_leaf_pane = Vec::new();
        let mut visible_indices = Vec::new();

        let rendered_children = self
            .members
            .iter()
            .enumerate()
            .filter_map(|(ix, member)| {
                if !member.is_visible(cx) {
                    member.clear_bounding_boxes();
                    return None;
                }

                let visible_ix = visible_indices.len();
                visible_indices.push(ix);
                match member {
                    Member::Pane(pane) => {
                        is_leaf_pane.push(true);
                        if pane == render_cx.active_pane() {
                            active_pane_ix = Some(visible_ix);
                            contains_active_pane = true;
                        }
                    }
                    Member::Axis(_) => {
                        is_leaf_pane.push(false);
                    }
                }

                let result = member.render((basis + ix) * 10, zoomed, render_cx, window, cx);
                if result.contains_active_pane {
                    contains_active_pane = true;
                }
                Some(result.element.into_any_element())
            })
            .collect::<Vec<_>>();

        if rendered_children.is_empty() {
            return PaneRenderResult {
                element: div().into_any(),
                contains_active_pane: false,
            };
        }

        let element = pane_axis(
            self.axis,
            basis,
            self.flexes.clone(),
            self.bounding_boxes.clone(),
            render_cx.workspace().clone(),
        )
        .with_visible_indices(visible_indices)
        .with_is_leaf_pane_mask(is_leaf_pane)
        .children(rendered_children)
        .with_active_pane(active_pane_ix)
        .into_any_element();

        PaneRenderResult {
            element,
            contains_active_pane,
        }
    }
}
