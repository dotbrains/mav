use super::*;

pub(super) struct Gutter<'a> {
    pub(super) line_height: Pixels,
    pub(super) range: Range<DisplayRow>,
    pub(super) scroll_position: gpui::Point<ScrollOffset>,
    pub(super) dimensions: &'a GutterDimensions,
    pub(super) hitbox: &'a Hitbox,
    pub(super) snapshot: &'a EditorSnapshot,
    pub(super) row_infos: &'a [RowInfo],
}

impl Gutter<'_> {
    pub(super) fn layout_item_skipping_folds(
        &self,
        display_row: DisplayRow,
        render_item: impl Fn(&mut Context<'_, Editor>, &mut Window) -> AnyElement,
        window: &mut Window,
        cx: &mut Context<'_, Editor>,
    ) -> Option<AnyElement> {
        let row = MultiBufferRow(
            DisplayPoint::new(display_row, 0)
                .to_point(self.snapshot)
                .row,
        );
        if self.snapshot.is_line_folded(row) {
            return None;
        }

        self.layout_item(display_row, render_item, window, cx)
    }

    pub(super) fn layout_item(
        &self,
        display_row: DisplayRow,
        render_item: impl Fn(&mut Context<'_, Editor>, &mut Window) -> AnyElement,
        window: &mut Window,
        cx: &mut Context<'_, Editor>,
    ) -> Option<AnyElement> {
        if !self.range.contains(&display_row) {
            return None;
        }

        if self
            .row_infos
            .get((display_row.0.saturating_sub(self.range.start.0)) as usize)
            .is_some_and(|row_info| {
                row_info.expand_info.is_some()
                    || row_info
                        .diff_status
                        .is_some_and(|status| status.is_deleted())
            })
        {
            return None;
        }

        let button = self.prepaint_button(render_item(cx, window), display_row, window, cx);
        Some(button)
    }

    pub(super) fn prepaint_button(
        &self,
        mut button: AnyElement,
        row: DisplayRow,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let available_space = size(
            AvailableSpace::MinContent,
            AvailableSpace::Definite(self.line_height),
        );
        let indicator_size = button.layout_as_root(available_space, window, cx);
        let git_gutter_width = EditorElement::gutter_strip_width(self.line_height)
            + self.dimensions.git_blame_entries_width.unwrap_or_default();

        let x = git_gutter_width + px(2.);

        let mut y = Pixels::from(
            (row.as_f64() - self.scroll_position.y) * ScrollPixelOffset::from(self.line_height),
        );
        y += (self.line_height - indicator_size.height) / 2.;

        button.prepaint_as_root(
            self.hitbox.origin + point(x, y),
            available_space,
            window,
            cx,
        );
        button
    }
}

impl EditorElement {
    pub(super) fn layout_gutter_diff_hunks(
        &self,
        line_height: Pixels,
        gutter_hitbox: &Hitbox,
        display_rows: Range<DisplayRow>,
        snapshot: &EditorSnapshot,
        scroll_position: gpui::Point<ScrollOffset>,
        window: &mut Window,
        cx: &mut App,
    ) -> Vec<(DisplayDiffHunk, Option<Hitbox>)> {
        let folded_buffers = self.editor.read(cx).folded_buffers(cx);
        let mut display_hunks = snapshot
            .display_diff_hunks_for_rows(display_rows, folded_buffers)
            .map(|hunk| (hunk, None))
            .collect::<Vec<_>>();
        let git_gutter_setting = ProjectSettings::get_global(cx).git.git_gutter;
        if let GitGutterSetting::TrackedFiles = git_gutter_setting {
            for (hunk, hitbox) in &mut display_hunks {
                if matches!(hunk, DisplayDiffHunk::Unfolded { .. }) {
                    let hunk_bounds = Self::diff_hunk_bounds(
                        scroll_position,
                        line_height,
                        gutter_hitbox.bounds,
                        hunk,
                        snapshot,
                    );
                    *hitbox = Some(window.insert_hitbox(hunk_bounds, HitboxBehavior::BlockMouse));
                }
            }
        }

        display_hunks
    }
}
