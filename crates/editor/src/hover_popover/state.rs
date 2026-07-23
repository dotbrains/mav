use super::*;

#[derive(Default)]
pub struct HoverState {
    pub info_popovers: Vec<InfoPopover>,
    pub diagnostic_popover: Option<DiagnosticPopover>,
    pub info_task: Option<Task<Option<()>>>,
    pub closest_mouse_distance: Option<Pixels>,
    pub hiding_delay_task: Option<Task<()>>,
}

impl HoverState {
    pub fn visible(&self) -> bool {
        !self.info_popovers.is_empty() || self.diagnostic_popover.is_some()
    }

    pub fn is_mouse_getting_closer(&mut self, mouse_position: gpui::Point<Pixels>) -> bool {
        if !self.visible() {
            return false;
        }

        let mut popover_bounds = Vec::new();
        for info_popover in &self.info_popovers {
            if let Some(bounds) = info_popover.last_bounds.get() {
                popover_bounds.push(bounds);
            }
        }
        if let Some(diagnostic_popover) = &self.diagnostic_popover {
            if let Some(bounds) = diagnostic_popover.last_bounds.get() {
                popover_bounds.push(bounds);
            }
        }

        if popover_bounds.is_empty() {
            return false;
        }

        let distance = popover_bounds
            .iter()
            .map(|bounds| self.distance_from_point_to_bounds(mouse_position, *bounds))
            .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .unwrap_or(px(f32::MAX));

        if let Some(closest_distance) = self.closest_mouse_distance {
            if distance > closest_distance + px(4.0) {
                return false;
            }
        }

        self.closest_mouse_distance =
            Some(distance.min(self.closest_mouse_distance.unwrap_or(distance)));
        true
    }

    fn distance_from_point_to_bounds(
        &self,
        point: gpui::Point<Pixels>,
        bounds: Bounds<Pixels>,
    ) -> Pixels {
        let center_x = bounds.origin.x + bounds.size.width / 2.;
        let center_y = bounds.origin.y + bounds.size.height / 2.;
        let dx: f32 = ((point.x - center_x).abs() - bounds.size.width / 2.)
            .max(px(0.0))
            .into();
        let dy: f32 = ((point.y - center_y).abs() - bounds.size.height / 2.)
            .max(px(0.0))
            .into();
        px((dx.powi(2) + dy.powi(2)).sqrt())
    }

    pub(crate) fn render(
        &mut self,
        snapshot: &EditorSnapshot,
        visible_rows: Range<DisplayRow>,
        max_size: Size<Pixels>,
        text_layout_details: &TextLayoutDetails,
        window: &mut Window,
        cx: &mut Context<Editor>,
    ) -> Option<(DisplayPoint, Vec<AnyElement>)> {
        if !self.visible() {
            return None;
        }
        // If there is a diagnostic, position the popovers based on that.
        // Otherwise use the start of the hover range
        let anchor = self
            .diagnostic_popover
            .as_ref()
            .map(|diagnostic_popover| &diagnostic_popover.local_diagnostic.range.start)
            .or_else(|| {
                self.info_popovers.iter().find_map(|info_popover| {
                    match &info_popover.symbol_range {
                        RangeInEditor::Text(range) => Some(&range.start),
                        RangeInEditor::Inlay(_) => None,
                    }
                })
            })
            .or_else(|| {
                self.info_popovers.iter().find_map(|info_popover| {
                    match &info_popover.symbol_range {
                        RangeInEditor::Text(_) => None,
                        RangeInEditor::Inlay(range) => Some(&range.inlay_position),
                    }
                })
            })?;
        let mut point = anchor.to_display_point(&snapshot.display_snapshot);
        // Clamp the point within the visible rows in case the popup source spans multiple lines
        if visible_rows.end <= point.row() {
            point = crate::movement::up_by_rows(
                &snapshot.display_snapshot,
                point,
                1 + (point.row() - visible_rows.end).0,
                text::SelectionGoal::None,
                true,
                text_layout_details,
            )
            .0;
        } else if point.row() < visible_rows.start {
            point = crate::movement::down_by_rows(
                &snapshot.display_snapshot,
                point,
                (visible_rows.start - point.row()).0,
                text::SelectionGoal::None,
                true,
                text_layout_details,
            )
            .0;
        }

        if !visible_rows.contains(&point.row()) {
            log::error!("Hover popover point out of bounds after moving");
            return None;
        }

        let mut elements = Vec::new();

        if let Some(diagnostic_popover) = self.diagnostic_popover.as_ref() {
            elements.push(diagnostic_popover.render(max_size, window, cx));
        }
        for info_popover in &mut self.info_popovers {
            elements.push(info_popover.render(max_size, window, cx));
        }

        Some((point, elements))
    }

    pub fn focused(&self, window: &mut Window, cx: &mut Context<Editor>) -> bool {
        let mut hover_popover_is_focused = false;
        for info_popover in &self.info_popovers {
            if let Some(markdown_view) = &info_popover.parsed_content
                && markdown_view.focus_handle(cx).is_focused(window)
            {
                hover_popover_is_focused = true;
            }
        }
        if let Some(diagnostic_popover) = &self.diagnostic_popover
            && diagnostic_popover
                .markdown
                .focus_handle(cx)
                .is_focused(window)
        {
            hover_popover_is_focused = true;
        }
        hover_popover_is_focused
    }
}
