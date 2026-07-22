use super::*;

impl GitGraph {
    pub(super) fn commit_count_and_loading_state(
        &mut self,
        cx: &mut Context<Self>,
    ) -> (usize, bool) {
        match self.graph_data.max_commit_count {
            AllCommitCount::FullyLoaded(count) => (count, false),
            AllCommitCount::Loading(count) => {
                let is_loading = self
                    .get_repository(cx)
                    .map(|repository| {
                        repository.update(cx, |repository, cx| {
                            repository
                                .graph_data(self.log_source.clone(), self.log_order, 0..0, cx)
                                .is_loading
                        })
                    })
                    .unwrap_or(false);

                (count, is_loading)
            }
            AllCommitCount::NotLoaded => {
                let (commit_count, is_loading) = if let Some(repository) = self.get_repository(cx) {
                    repository.update(cx, |repository, cx| {
                        // Start loading the graph data if we haven't started already
                        let GraphDataResponse {
                            commits,
                            is_loading,
                            error: _,
                        } = repository.graph_data(
                            self.log_source.clone(),
                            self.log_order,
                            0..usize::MAX,
                            cx,
                        );
                        self.graph_data.add_commits(commits);
                        (commits.len(), is_loading)
                    })
                } else {
                    (0, false)
                };

                (commit_count, is_loading)
            }
        }
    }

    pub(super) fn render_commit_view_resize_handle(
        &self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .id("commit-view-split-resize-container")
            .relative()
            .h_full()
            .flex_shrink_0()
            .w(px(1.))
            .bg(cx.theme().colors().border_variant)
            .child(
                div()
                    .id("commit-view-split-resize-handle")
                    .absolute()
                    .left(px(-RESIZE_HANDLE_WIDTH / 2.0))
                    .w(px(RESIZE_HANDLE_WIDTH))
                    .h_full()
                    .cursor_col_resize()
                    .block_mouse_except_scroll()
                    .on_click(cx.listener(|this, event: &ClickEvent, _window, cx| {
                        if event.click_count() >= 2 {
                            this.commit_details_split_state.update(cx, |state, _| {
                                state.on_double_click();
                            });
                        }
                        cx.stop_propagation();
                    }))
                    .on_drag(DraggedSplitHandle, |_, _, _, cx| cx.new(|_| gpui::Empty)),
            )
            .into_any_element()
    }
}
