use super::*;

impl Editor {
    pub(crate) fn active_bookmarks(
        &self,
        range: Range<DisplayRow>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> HashSet<DisplayRow> {
        let mut bookmark_display_points = HashSet::default();

        let Some(bookmark_store) = self.bookmark_store.clone() else {
            return bookmark_display_points;
        };

        let snapshot = self.snapshot(window, cx);

        let multi_buffer_snapshot = snapshot.buffer_snapshot();
        let Some(project) = self.project() else {
            return bookmark_display_points;
        };

        let range = snapshot.display_point_to_point(DisplayPoint::new(range.start, 0), Bias::Left)
            ..snapshot.display_point_to_point(DisplayPoint::new(range.end, 0), Bias::Right);

        for (buffer_snapshot, range, _excerpt_range) in
            multi_buffer_snapshot.range_to_buffer_ranges(range.start..range.end)
        {
            let Some(buffer) = project
                .read(cx)
                .buffer_for_id(buffer_snapshot.remote_id(), cx)
            else {
                continue;
            };
            let bookmarks = bookmark_store.update(cx, |store, cx| {
                store.bookmarks_for_buffer(
                    buffer,
                    buffer_snapshot.anchor_before(range.start)
                        ..buffer_snapshot.anchor_after(range.end),
                    &buffer_snapshot,
                    cx,
                )
            });
            for bookmark in bookmarks {
                let Some(multi_buffer_anchor) =
                    multi_buffer_snapshot.anchor_in_buffer(bookmark.anchor)
                else {
                    continue;
                };
                let position = multi_buffer_anchor
                    .to_point(&multi_buffer_snapshot)
                    .to_display_point(&snapshot);

                bookmark_display_points.insert(position.row());
            }
        }

        bookmark_display_points
    }

    pub(crate) fn render_bookmark(&self, row: DisplayRow, cx: &mut Context<Self>) -> IconButton {
        let focus_handle = self.focus_handle.clone();
        IconButton::new(("bookmark indicator", row.0 as usize), IconName::Bookmark)
            .icon_size(IconSize::XSmall)
            .size(ui::ButtonSize::None)
            .icon_color(Color::Info)
            .style(ButtonStyle::Transparent)
            .on_click(cx.listener(move |editor, _, window, cx| {
                editor.toggle_bookmark_at_row(row, window, cx);
            }))
            .on_right_click(cx.listener(move |editor, event: &ClickEvent, window, cx| {
                editor.set_gutter_context_menu(row, None, event.position(), window, cx);
            }))
            .tooltip(move |_window, cx| {
                Tooltip::with_meta_in(
                    "Remove Bookmark",
                    Some(&ToggleBookmark),
                    SharedString::from("Right-click for more options"),
                    &focus_handle,
                    cx,
                )
            })
    }

    pub(crate) fn bookmark_at_row(
        &self,
        row: u32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Anchor> {
        let snapshot = self.snapshot(window, cx);
        let bookmark_position = snapshot.buffer_snapshot().anchor_before(Point::new(row, 0));

        self.bookmark_at_anchor(bookmark_position, &snapshot, cx)
    }

    pub(crate) fn bookmark_at_anchor(
        &self,
        bookmark_position: Anchor,
        snapshot: &EditorSnapshot,
        cx: &mut Context<Self>,
    ) -> Option<Anchor> {
        let (bookmark_position, _) = snapshot
            .buffer_snapshot()
            .anchor_to_buffer_anchor(bookmark_position)?;
        let buffer = self.buffer.read(cx).buffer(bookmark_position.buffer_id)?;

        let buffer_snapshot = buffer.read(cx).snapshot();

        let row = buffer_snapshot
            .summary_for_anchor::<text::PointUtf16>(&bookmark_position)
            .row;

        let line_len = buffer_snapshot.line_len(row);
        let anchor_end = buffer_snapshot.anchor_after(Point::new(row, line_len));

        self.bookmark_store
            .as_ref()?
            .update(cx, |bookmark_store, cx| {
                bookmark_store
                    .bookmarks_for_buffer(
                        buffer,
                        bookmark_position..anchor_end,
                        &buffer_snapshot,
                        cx,
                    )
                    .first()
                    .and_then(|bookmark| {
                        let bookmark_row = buffer_snapshot
                            .summary_for_anchor::<text::PointUtf16>(&bookmark.anchor)
                            .row;

                        if bookmark_row == row {
                            snapshot
                                .buffer_snapshot()
                                .anchor_in_excerpt(bookmark.anchor)
                        } else {
                            None
                        }
                    })
            })
    }
}
