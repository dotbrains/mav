use super::*;

impl Editor {
    pub fn set_searchable(&mut self, searchable: bool) {
        self.searchable = searchable;
    }

    pub fn searchable(&self) -> bool {
        self.searchable
    }

    pub fn open_excerpts_in_split(
        &mut self,
        _: &OpenExcerptsSplit,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_excerpts_common(None, true, window, cx)
    }

    pub fn open_excerpts(&mut self, _: &OpenExcerpts, window: &mut Window, cx: &mut Context<Self>) {
        self.open_excerpts_common(None, false, window, cx)
    }

    pub(crate) fn open_excerpts_common(
        &mut self,
        jump_data: Option<JumpData>,
        split: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.buffer.read(cx).is_singleton() {
            cx.propagate();
            return;
        }

        let mut new_selections_by_buffer = HashMap::default();
        match &jump_data {
            Some(JumpData::MultiBufferPoint {
                anchor,
                position,
                line_offset_from_top,
            }) => {
                if let Some(buffer) = self.buffer.read(cx).buffer(anchor.buffer_id) {
                    let buffer_snapshot = buffer.read(cx).snapshot();
                    let jump_to_point = if buffer_snapshot.can_resolve(&anchor) {
                        language::ToPoint::to_point(anchor, &buffer_snapshot)
                    } else {
                        buffer_snapshot.clip_point(*position, Bias::Left)
                    };
                    let jump_to_offset = buffer_snapshot.point_to_offset(jump_to_point);
                    new_selections_by_buffer.insert(
                        buffer,
                        (
                            vec![BufferOffset(jump_to_offset)..BufferOffset(jump_to_offset)],
                            Some(*line_offset_from_top),
                        ),
                    );
                }
            }
            Some(JumpData::MultiBufferRow {
                row,
                line_offset_from_top,
            }) => {
                let point = MultiBufferPoint::new(row.0, 0);
                if let Some((buffer, buffer_point)) =
                    self.buffer.read(cx).point_to_buffer_point(point, cx)
                {
                    let buffer_offset = buffer.read(cx).point_to_offset(buffer_point);
                    new_selections_by_buffer
                        .entry(buffer)
                        .or_insert((Vec::new(), Some(*line_offset_from_top)))
                        .0
                        .push(BufferOffset(buffer_offset)..BufferOffset(buffer_offset))
                }
            }
            None => {
                let selections = self
                    .selections
                    .all::<MultiBufferOffset>(&self.display_snapshot(cx));
                let multi_buffer = self.buffer.read(cx);
                let multi_buffer_snapshot = multi_buffer.snapshot(cx);
                for selection in selections {
                    for (snapshot, range, anchor) in multi_buffer_snapshot
                        .range_to_buffer_ranges_with_deleted_hunks(selection.range())
                    {
                        if let Some((text_anchor, _)) = anchor.and_then(|anchor| {
                            multi_buffer_snapshot.anchor_to_buffer_anchor(anchor)
                        }) {
                            let Some(buffer_handle) = multi_buffer.buffer(text_anchor.buffer_id)
                            else {
                                continue;
                            };
                            let offset = text::ToOffset::to_offset(
                                &text_anchor,
                                &buffer_handle.read(cx).snapshot(),
                            );
                            let range = BufferOffset(offset)..BufferOffset(offset);
                            new_selections_by_buffer
                                .entry(buffer_handle)
                                .or_insert((Vec::new(), None))
                                .0
                                .push(range)
                        } else {
                            let Some(buffer_handle) = multi_buffer.buffer(snapshot.remote_id())
                            else {
                                continue;
                            };
                            new_selections_by_buffer
                                .entry(buffer_handle)
                                .or_insert((Vec::new(), None))
                                .0
                                .push(range)
                        }
                    }
                }
            }
        }

        if self.delegate_open_excerpts {
            let selections_by_buffer: HashMap<_, _> = new_selections_by_buffer
                .into_iter()
                .map(|(buffer, value)| (buffer.read(cx).remote_id(), value))
                .collect();
            if !selections_by_buffer.is_empty() {
                cx.emit(EditorEvent::OpenExcerptsRequested {
                    selections_by_buffer,
                    split,
                });
            }
            return;
        }

        let Some(workspace) = self.workspace() else {
            cx.propagate();
            return;
        };

        new_selections_by_buffer
            .retain(|buffer, _| buffer.read(cx).file().is_none_or(|file| file.can_open()));

        if new_selections_by_buffer.is_empty() {
            return;
        }

        Self::open_buffers_in_workspace(
            workspace.downgrade(),
            new_selections_by_buffer,
            split,
            window,
            cx,
        );
    }

    pub(crate) fn open_buffers_in_workspace(
        workspace: WeakEntity<Workspace>,
        new_selections_by_buffer: HashMap<
            Entity<language::Buffer>,
            (Vec<Range<BufferOffset>>, Option<u32>),
        >,
        split: bool,
        window: &mut Window,
        cx: &mut App,
    ) {
        window.defer(cx, move |window, cx| {
            workspace
                .update(cx, |workspace, cx| {
                    let pane = if split {
                        workspace.adjacent_pane(window, cx)
                    } else {
                        workspace.active_pane().clone()
                    };

                    for (buffer, (ranges, scroll_offset)) in new_selections_by_buffer {
                        let buffer_read = buffer.read(cx);
                        let (has_file, is_project_file) = if let Some(file) = buffer_read.file() {
                            (true, project::File::from_dyn(Some(file)).is_some())
                        } else {
                            (false, false)
                        };

                        let editor = (!has_file || !is_project_file)
                            .then(|| {
                                let (editor, pane_item_index, pane_item_id) =
                                    pane.read(cx).items().enumerate().find_map(|(i, item)| {
                                        let editor = item.downcast::<Editor>()?;
                                        let singleton_buffer =
                                            editor.read(cx).buffer().read(cx).as_singleton()?;
                                        if singleton_buffer == buffer {
                                            Some((editor, i, item.item_id()))
                                        } else {
                                            None
                                        }
                                    })?;
                                pane.update(cx, |pane, cx| {
                                    pane.activate_item(pane_item_index, true, true, window, cx);
                                    if !PreviewTabsSettings::get_global(cx)
                                        .enable_preview_from_multibuffer
                                    {
                                        pane.unpreview_item_if_preview(pane_item_id);
                                    }
                                });
                                Some(editor)
                            })
                            .flatten()
                            .unwrap_or_else(|| {
                                let keep_old_preview = PreviewTabsSettings::get_global(cx)
                                    .enable_keep_preview_on_code_navigation;
                                let allow_new_preview = PreviewTabsSettings::get_global(cx)
                                    .enable_preview_from_multibuffer;
                                workspace.open_project_item::<Self>(
                                    pane.clone(),
                                    buffer,
                                    true,
                                    true,
                                    keep_old_preview,
                                    allow_new_preview,
                                    window,
                                    cx,
                                )
                            });

                        editor.update(cx, |editor, cx| {
                            if has_file && !is_project_file {
                                editor.set_read_only(true);
                            }
                            let autoscroll = match scroll_offset {
                                Some(scroll_offset) => {
                                    Autoscroll::top_relative(scroll_offset as ScrollOffset)
                                }
                                None => Autoscroll::newest(),
                            };
                            let nav_history = editor.nav_history.take();
                            let multibuffer_snapshot = editor.buffer().read(cx).snapshot(cx);
                            let Some(buffer_snapshot) = multibuffer_snapshot.as_singleton() else {
                                return;
                            };
                            editor.change_selections(
                                SelectionEffects::scroll(autoscroll),
                                window,
                                cx,
                                |s| {
                                    s.select_ranges(ranges.into_iter().map(|range| {
                                        let range = buffer_snapshot.anchor_before(range.start)
                                            ..buffer_snapshot.anchor_after(range.end);
                                        multibuffer_snapshot
                                            .buffer_anchor_range_to_anchor_range(range)
                                            .unwrap()
                                    }));
                                },
                            );
                            editor.nav_history = nav_history;
                        });
                    }
                })
                .ok();
        });
    }
}
