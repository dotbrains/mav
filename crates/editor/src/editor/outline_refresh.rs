use super::*;

impl Editor {
    #[ztracing::instrument(skip_all)]
    pub(crate) fn refresh_outline_symbols_at_cursor(&mut self, cx: &mut Context<Editor>) {
        if !self.lsp_data_enabled() {
            return;
        }
        let cursor = self.selections.newest_anchor().head();
        let multi_buffer_snapshot = self.buffer().read(cx).snapshot(cx);

        if self.uses_lsp_document_symbols(cursor, &multi_buffer_snapshot, cx) {
            self.outline_symbols_at_cursor =
                self.lsp_symbols_at_cursor(cursor, &multi_buffer_snapshot, cx);
            cx.emit(EditorEvent::OutlineSymbolsChanged);
            cx.notify();
        } else {
            let syntax = cx.theme().syntax().clone();
            let background_task = cx.background_spawn(async move {
                multi_buffer_snapshot.symbols_containing(cursor, Some(&syntax))
            });
            self.refresh_outline_symbols_at_cursor_at_cursor_task =
                cx.spawn(async move |this, cx| {
                    let symbols = background_task.await;
                    this.update(cx, |this, cx| {
                        this.outline_symbols_at_cursor = symbols;
                        cx.emit(EditorEvent::OutlineSymbolsChanged);
                        cx.notify();
                    })
                    .ok();
                });
        }
    }

    pub fn multi_buffer_visible_range(
        &self,
        display_snapshot: &DisplaySnapshot,
        cx: &App,
    ) -> Range<Point> {
        let visible_start = self
            .scroll_manager
            .native_anchor(display_snapshot, cx)
            .anchor
            .to_point(display_snapshot.buffer_snapshot())
            .to_display_point(display_snapshot);

        let mut target_end = visible_start;
        *target_end.row_mut() += self.visible_line_count().unwrap_or(0.).ceil() as u32;

        visible_start.to_point(display_snapshot)
            ..display_snapshot
                .clip_point(target_end, Bias::Right)
                .to_point(display_snapshot)
    }
}
