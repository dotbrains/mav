use super::sizing::{normalize_flexes, split_ratios_for_inserted_size};
use super::*;

#[derive(Debug, Clone)]
pub struct PaneAxis {
    pub axis: Axis,
    pub members: Vec<Member>,
    pub flexes: Arc<Mutex<Vec<f32>>>,
    pub bounding_boxes: Arc<Mutex<Vec<Option<Bounds<Pixels>>>>>,
}

impl PaneAxis {
    pub fn new(axis: Axis, members: Vec<Member>) -> Self {
        let flexes = Arc::new(Mutex::new(vec![1.; members.len()]));
        let bounding_boxes = Arc::new(Mutex::new(vec![None; members.len()]));
        Self {
            axis,
            members,
            flexes,
            bounding_boxes,
        }
    }

    pub fn load(axis: Axis, members: Vec<Member>, flexes: Option<Vec<f32>>) -> Self {
        let flexes = normalize_flexes(members.len(), flexes.unwrap_or_default());
        let flexes = Arc::new(Mutex::new(flexes));
        let bounding_boxes = Arc::new(Mutex::new(vec![None; members.len()]));
        let mut axis = Self {
            axis,
            members,
            flexes,
            bounding_boxes,
        };
        axis.normalize_same_axis();
        axis
    }

    pub(super) fn split(
        &mut self,
        old_pane: &Entity<Pane>,
        new_pane: &Entity<Pane>,
        direction: SplitDirection,
        size_hint: Option<SplitSizeHint>,
        cx: &App,
    ) -> bool {
        let bounding_boxes = self.bounding_boxes.lock().clone();
        for (idx, member) in self.members.iter_mut().enumerate() {
            match member {
                Member::Axis(axis) => {
                    if axis.split(old_pane, new_pane, direction, size_hint, cx) {
                        return true;
                    }
                }
                Member::Pane(pane) => {
                    if pane == old_pane {
                        if direction.axis() == self.axis {
                            let insertion_ix = if direction.increasing() { idx + 1 } else { idx };
                            self.insert_pane(insertion_ix, idx, new_pane, size_hint, cx);
                        } else {
                            let available_size = bounding_boxes
                                .get(idx)
                                .and_then(|bounds| {
                                    bounds.map(|bounds| bounds.size.along(direction.axis()))
                                })
                                .map(|size| Pixels::max(size - workspace_card_gap(cx), px(0.)));
                            *member = Member::new_axis_with_size_hint(
                                old_pane.clone(),
                                new_pane.clone(),
                                direction,
                                size_hint,
                                available_size,
                            );
                        }
                        return true;
                    }
                }
            }
        }
        false
    }

    fn insert_pane(
        &mut self,
        idx: usize,
        split_ix: usize,
        new_pane: &Entity<Pane>,
        size_hint: Option<SplitSizeHint>,
        cx: &App,
    ) {
        let visible_members_are_distributed = self.visible_members_are_distributed(cx);
        let mut flexes = normalize_flexes(self.members.len(), self.flexes.lock().clone());

        self.members.insert(idx, Member::Pane(new_pane.clone()));

        if let Some((old_flex, inserted_flex)) =
            self.split_flex_for_inserted_size(split_ix, size_hint, &flexes, cx)
        {
            if let Some(flex) = flexes.get_mut(split_ix) {
                *flex = old_flex;
            }
            flexes.insert(idx, inserted_flex);
        } else if visible_members_are_distributed {
            flexes.insert(idx, 1.);
            for ix in self.visible_member_indices(cx) {
                flexes[ix] = 1.;
            }
        } else {
            let split_flex = flexes
                .get(split_ix)
                .copied()
                .unwrap_or(1.)
                .max(f32::EPSILON)
                / 2.;
            if let Some(flex) = flexes.get_mut(split_ix) {
                *flex = split_flex;
            }
            flexes.insert(idx, split_flex);
        }

        *self.flexes.lock() = flexes;
        *self.bounding_boxes.lock() = vec![None; self.members.len()];
    }

    pub(super) fn split_flex_for_inserted_size(
        &self,
        split_ix: usize,
        size_hint: Option<SplitSizeHint>,
        flexes: &[f32],
        cx: &App,
    ) -> Option<(f32, f32)> {
        if self.axis != Axis::Horizontal {
            return None;
        }

        let hint = size_hint?;
        let split_size = hint.available_size.or_else(|| {
            self.bounding_boxes
                .lock()
                .get(split_ix)
                .and_then(|bounds| *bounds)
                .map(|bounds| bounds.size.width - workspace_card_gap(cx))
        })?;
        let (old_ratio, inserted_ratio) =
            split_ratios_for_inserted_size(split_size, hint.inserted_size)?;
        let split_flex = flexes.get(split_ix).copied()?.max(f32::EPSILON);
        Some((split_flex * old_ratio, split_flex * inserted_ratio))
    }

    pub(super) fn insert_moved_pane(&mut self, idx: usize, pane: &Entity<Pane>) {
        self.members.insert(idx, Member::Pane(pane.clone()));
        let mut flexes = normalize_flexes(
            self.members.len().saturating_sub(1),
            self.flexes.lock().clone(),
        );
        flexes.insert(idx, 1.);
        *self.flexes.lock() = flexes;
        *self.bounding_boxes.lock() = vec![None; self.members.len()];
    }

    pub(super) fn find_pane_at_border(&self, direction: SplitDirection) -> Option<&Entity<Pane>> {
        if self.axis != direction.axis() {
            return None;
        }
        let member = if direction.increasing() {
            self.members.last()
        } else {
            self.members.first()
        };
        member.and_then(|e| match e {
            Member::Pane(pane) => Some(pane),
            Member::Axis(_) => None,
        })
    }

    pub(super) fn horizontal_size_for_pane(&self, pane: &Entity<Pane>) -> Option<Pixels> {
        let bounding_boxes = self.bounding_boxes.lock();

        for (idx, member) in self.members.iter().enumerate() {
            if self.axis == Axis::Horizontal && member.contains_pane(pane) {
                return bounding_boxes
                    .get(idx)
                    .and_then(|bounds| bounds.map(|bounds| bounds.size.width));
            }

            if let Member::Axis(axis) = member
                && let Some(size) = axis.horizontal_size_for_pane(pane)
            {
                return Some(size);
            }
        }

        None
    }

    pub(super) fn remove(
        &mut self,
        pane_to_remove: &Entity<Pane>,
        cx: &App,
    ) -> Result<Option<Member>> {
        let mut found_pane = false;
        let mut remove_member = None;
        for (idx, member) in self.members.iter_mut().enumerate() {
            match member {
                Member::Axis(axis) => {
                    if let Ok(last_pane) = axis.remove(pane_to_remove, cx) {
                        if let Some(last_pane) = last_pane {
                            *member = last_pane;
                        }
                        found_pane = true;
                        break;
                    }
                }
                Member::Pane(pane) => {
                    if pane == pane_to_remove {
                        found_pane = true;
                        remove_member = Some(idx);
                        break;
                    }
                }
            }
        }

        if found_pane {
            if let Some(idx) = remove_member {
                let visible_members_are_distributed = self.visible_members_are_distributed(cx);
                let visible_indices = self.visible_member_indices(cx);
                let removed_was_visible = visible_indices.contains(&idx);
                let mut flexes = normalize_flexes(self.members.len(), self.flexes.lock().clone());
                let removed_flex = flexes.get(idx).copied().unwrap_or(1.);
                let recipient_ix = if removed_was_visible && !visible_members_are_distributed {
                    visible_indices
                        .iter()
                        .rev()
                        .copied()
                        .find(|visible_ix| *visible_ix < idx)
                        .or_else(|| {
                            visible_indices
                                .iter()
                                .copied()
                                .find(|visible_ix| *visible_ix > idx)
                        })
                } else {
                    None
                };

                self.members.remove(idx);
                if idx < flexes.len() {
                    flexes.remove(idx);
                }

                if visible_members_are_distributed {
                    for ix in self.visible_member_indices(cx) {
                        flexes[ix] = 1.;
                    }
                } else if let Some(recipient_ix) = recipient_ix {
                    let recipient_ix = if recipient_ix > idx {
                        recipient_ix - 1
                    } else {
                        recipient_ix
                    };
                    if let Some(flex) = flexes.get_mut(recipient_ix) {
                        *flex += removed_flex;
                    }
                }

                *self.flexes.lock() = flexes;
                *self.bounding_boxes.lock() = vec![None; self.members.len()];
            }

            if self.members.len() == 1 {
                let result = self.members.pop();
                *self.flexes.lock() = vec![1.; self.members.len()];
                Ok(result)
            } else {
                Ok(None)
            }
        } else {
            anyhow::bail!("Pane not found");
        }
    }

    pub(super) fn reset_pane_sizes(&self) {
        *self.flexes.lock() = vec![1.; self.members.len()];
        for member in self.members.iter() {
            if let Member::Axis(axis) = member {
                axis.reset_pane_sizes();
            }
        }
    }

    fn visible_member_indices(&self, cx: &App) -> Vec<usize> {
        self.members
            .iter()
            .enumerate()
            .filter_map(|(ix, member)| member.is_visible(cx).then_some(ix))
            .collect()
    }

    fn visible_members_are_distributed(&self, cx: &App) -> bool {
        const DISTRIBUTED_SIZE_TOLERANCE: f32 = 2.;

        let visible_indices = self.visible_member_indices(cx);
        if visible_indices.len() <= 1 {
            return true;
        }

        let bounding_boxes = self.bounding_boxes.lock();
        let rendered_sizes = visible_indices
            .iter()
            .map(|ix| {
                bounding_boxes
                    .get(*ix)
                    .and_then(|bounds| bounds.map(|bounds| bounds.size.along(self.axis).as_f32()))
            })
            .collect::<Option<Vec<_>>>();

        if let Some(rendered_sizes) = rendered_sizes {
            let (min_size, max_size) = rendered_sizes
                .iter()
                .fold((f32::INFINITY, f32::NEG_INFINITY), |(min, max), size| {
                    (min.min(*size), max.max(*size))
                });
            return max_size - min_size <= DISTRIBUTED_SIZE_TOLERANCE;
        }

        let flexes = normalize_flexes(self.members.len(), self.flexes.lock().clone());
        let (min_flex, max_flex) = visible_indices
            .iter()
            .filter_map(|ix| flexes.get(*ix))
            .fold((f32::INFINITY, f32::NEG_INFINITY), |(min, max), flex| {
                (min.min(*flex), max.max(*flex))
            });

        (max_flex - min_flex).abs() <= f32::EPSILON
    }

    pub(super) fn normalize_same_axis(&mut self) {
        for member in &mut self.members {
            member.normalize_same_axis();
        }

        if !self
            .members
            .iter()
            .any(|member| matches!(member, Member::Axis(axis) if axis.axis == self.axis))
        {
            let flexes = normalize_flexes(self.members.len(), self.flexes.lock().clone());
            *self.flexes.lock() = flexes;
            return;
        }

        let members = mem::take(&mut self.members);
        let flexes = normalize_flexes(members.len(), self.flexes.lock().clone());
        let mut flattened_members = Vec::with_capacity(members.len());
        let mut flattened_flexes = Vec::with_capacity(flexes.len());

        for (member, flex) in members.into_iter().zip(flexes) {
            match member {
                Member::Axis(axis) if axis.axis == self.axis => {
                    let child_flexes =
                        normalize_flexes(axis.members.len(), axis.flexes.lock().clone());
                    let child_total_flex = child_flexes.iter().sum::<f32>().max(0.001);
                    flattened_members.extend(axis.members);
                    flattened_flexes.extend(
                        child_flexes
                            .into_iter()
                            .map(|child_flex| flex * child_flex / child_total_flex),
                    );
                }
                member => {
                    flattened_members.push(member);
                    flattened_flexes.push(flex);
                }
            }
        }

        self.members = flattened_members;
        *self.flexes.lock() = normalize_flexes(self.members.len(), flattened_flexes);
        *self.bounding_boxes.lock() = vec![None; self.members.len()];
    }
}
