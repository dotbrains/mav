use super::{
    blame_entries::{render_blame_entry, render_blame_entry_popover, render_inline_blame_entry},
    *,
};
use crate::git::blame::GlobalBlameRenderer;
use git::Oid;

impl EditorElement {
    pub(super) fn layout_inline_blame(
        &self,
        display_row: DisplayRow,
        row_info: &RowInfo,
        line_layout: &LineWithInvisibles,
        crease_trailer: Option<&CreaseTrailerLayout>,
        em_width: Pixels,
        content_origin: gpui::Point<Pixels>,
        scroll_position: gpui::Point<ScrollOffset>,
        scroll_pixel_position: gpui::Point<ScrollPixelOffset>,
        line_height: Pixels,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<InlineBlameLayout> {
        if !self
            .editor
            .update(cx, |editor, cx| editor.render_git_blame_inline(window, cx))
        {
            return None;
        }

        let editor = self.editor.read(cx);
        let blame = editor.blame.clone()?;
        let padding = {
            const INLINE_ACCEPT_SUGGESTION_EM_WIDTHS: f32 = 14.;

            let mut padding = ProjectSettings::get_global(cx).git.inline_blame.padding as f32;

            if let Some(edit_prediction) = editor.active_edit_prediction.as_ref()
                && let EditPrediction::Edit {
                    display_mode: EditDisplayMode::TabAccept,
                    ..
                } = &edit_prediction.completion
            {
                padding += INLINE_ACCEPT_SUGGESTION_EM_WIDTHS
            }

            padding * em_width
        };

        let (buffer_id, entry) = blame
            .update(cx, |blame, cx| {
                blame.blame_for_rows(&[*row_info], cx).next()
            })
            .flatten()?;

        let mut element = render_inline_blame_entry(entry.clone(), &self.style, cx)?;

        let start_y =
            content_origin.y + line_height * ((display_row.as_f64() - scroll_position.y) as f32);

        let start_x = {
            let line_end = if let Some(crease_trailer) = crease_trailer {
                crease_trailer.bounds.right()
            } else {
                Pixels::from(
                    ScrollPixelOffset::from(content_origin.x + line_layout.width)
                        - scroll_pixel_position.x,
                )
            };

            let padded_line_end = line_end + padding;

            let min_column_in_pixels = column_pixels(
                &self.style,
                ProjectSettings::get_global(cx).git.inline_blame.min_column as usize,
                window,
            );
            let min_start = Pixels::from(
                ScrollPixelOffset::from(content_origin.x + min_column_in_pixels)
                    - scroll_pixel_position.x,
            );

            cmp::max(padded_line_end, min_start)
        };

        let absolute_offset = point(start_x, start_y);
        let size = element.layout_as_root(AvailableSpace::min_size(), window, cx);
        let bounds = Bounds::new(absolute_offset, size);

        element.prepaint_as_root(absolute_offset, AvailableSpace::min_size(), window, cx);

        Some(InlineBlameLayout {
            element,
            bounds,
            buffer_id,
            entry,
        })
    }

    pub(super) fn layout_blame_popover(
        &self,
        editor_snapshot: &EditorSnapshot,
        text_hitbox: &Hitbox,
        line_height: Pixels,
        window: &mut Window,
        cx: &mut App,
    ) {
        if !self.editor.read(cx).inline_blame_popover.is_some() {
            return;
        }

        let Some(blame) = self.editor.read(cx).blame.clone() else {
            return;
        };
        let cursor_point = self
            .editor
            .read(cx)
            .selections
            .newest::<language::Point>(&editor_snapshot.display_snapshot)
            .head();

        let Some((buffer, buffer_point)) = editor_snapshot
            .buffer_snapshot()
            .point_to_buffer_point(cursor_point)
        else {
            return;
        };

        let row_info = RowInfo {
            buffer_id: Some(buffer.remote_id()),
            buffer_row: Some(buffer_point.row),
            ..Default::default()
        };

        let Some((buffer_id, blame_entry)) = blame
            .update(cx, |blame, cx| blame.blame_for_rows(&[row_info], cx).next())
            .flatten()
        else {
            return;
        };

        let Some((popover_state, target_point)) = self.editor.read_with(cx, |editor, _| {
            editor
                .inline_blame_popover
                .as_ref()
                .map(|state| (state.popover_state.clone(), state.position))
        }) else {
            return;
        };

        let workspace = self
            .editor
            .read_with(cx, |editor, _| editor.workspace().map(|w| w.downgrade()));

        let maybe_element = workspace.and_then(|workspace| {
            render_blame_entry_popover(
                blame_entry,
                popover_state.scroll_handle,
                popover_state.commit_message,
                popover_state.markdown,
                workspace,
                &blame,
                buffer_id,
                window,
                cx,
            )
        });

        if let Some(mut element) = maybe_element {
            let size = element.layout_as_root(AvailableSpace::min_size(), window, cx);
            let overall_height = size.height + HOVER_POPOVER_GAP;
            let popover_origin = if target_point.y > overall_height {
                point(target_point.x, target_point.y - size.height)
            } else {
                point(
                    target_point.x,
                    target_point.y + line_height + HOVER_POPOVER_GAP,
                )
            };

            let horizontal_offset = (text_hitbox.top_right().x
                - POPOVER_RIGHT_OFFSET
                - (popover_origin.x + size.width))
                .min(Pixels::ZERO);

            let origin = point(popover_origin.x + horizontal_offset, popover_origin.y);
            let popover_bounds = Bounds::new(origin, size);

            self.editor.update(cx, |editor, _| {
                if let Some(state) = &mut editor.inline_blame_popover {
                    state.popover_bounds = Some(popover_bounds);
                }
            });

            window.defer_draw(element, origin, 2, None);
        }
    }

    pub(super) fn layout_blame_entries(
        &self,
        buffer_rows: &[RowInfo],
        em_width: Pixels,
        scroll_position: gpui::Point<ScrollOffset>,
        start_row: DisplayRow,
        line_height: Pixels,
        gutter_hitbox: &Hitbox,
        max_width: Option<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Vec<AnyElement>> {
        if !self
            .editor
            .update(cx, |editor, cx| editor.render_git_blame_gutter(cx))
        {
            return None;
        }

        let blame = self.editor.read(cx).blame.clone()?;
        let workspace = self.editor.read(cx).workspace()?;
        let blamed_rows: Vec<_> = blame.update(cx, |blame, cx| {
            blame.blame_for_rows(buffer_rows, cx).collect()
        });

        let width = if let Some(max_width) = max_width {
            AvailableSpace::Definite(max_width)
        } else {
            AvailableSpace::MaxContent
        };
        let start_x = em_width;

        let mut last_used_color: Option<(Hsla, Oid)> = None;
        let blame_renderer = cx.global::<GlobalBlameRenderer>().0.clone();

        let shaped_lines = blamed_rows
            .into_iter()
            .enumerate()
            .flat_map(|(ix, blame_entry)| {
                let (buffer_id, blame_entry) = blame_entry?;
                let mut element = render_blame_entry(
                    ix,
                    &blame,
                    blame_entry,
                    &self.style,
                    &mut last_used_color,
                    self.editor.clone(),
                    workspace.clone(),
                    buffer_id,
                    &*blame_renderer,
                    window,
                    cx,
                )?;

                let start_y = line_height
                    * (DisplayRow(start_row.0 + ix as u32).as_f64() - scroll_position.y) as f32;
                let absolute_offset = gutter_hitbox.origin + point(start_x, start_y);

                element.prepaint_as_root(
                    absolute_offset,
                    size(width, AvailableSpace::MinContent),
                    window,
                    cx,
                );

                Some(element)
            })
            .collect();

        Some(shaped_lines)
    }
}
