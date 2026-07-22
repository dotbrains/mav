use super::*;

impl StackFrameList {
    pub fn go_to_stack_frame(
        &mut self,
        stack_frame_id: StackFrameId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        let Some(stack_frame) = self
            .entries
            .iter()
            .flat_map(|entry| match entry {
                StackFrameEntry::Label(stack_frame) => std::slice::from_ref(stack_frame),
                StackFrameEntry::Normal(stack_frame) => std::slice::from_ref(stack_frame),
                StackFrameEntry::Collapsed(stack_frames) => stack_frames.as_slice(),
            })
            .find(|stack_frame| stack_frame.id == stack_frame_id)
            .cloned()
        else {
            return Task::ready(Err(anyhow!("No stack frame for ID")));
        };
        self.go_to_stack_frame_inner(stack_frame, window, cx)
    }

    pub(super) fn go_to_stack_frame_inner(
        &mut self,
        stack_frame: dap::StackFrame,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        let stack_frame_id = stack_frame.id;
        self.opened_stack_frame_id = Some(stack_frame_id);
        let Some(abs_path) = Self::abs_path_from_stack_frame(&stack_frame) else {
            return Task::ready(Err(anyhow!("Project path not found")));
        };
        let row = stack_frame.line.saturating_sub(1) as u32;
        cx.emit(StackFrameListEvent::SelectedStackFrameChanged(
            stack_frame_id,
        ));
        cx.spawn_in(window, async move |this, cx| {
            let (worktree, relative_path) = this
                .update(cx, |this, cx| {
                    this.workspace.update(cx, |workspace, cx| {
                        workspace.project().update(cx, |this, cx| {
                            this.find_or_create_worktree(&abs_path, false, cx)
                        })
                    })
                })??
                .await?;
            let buffer = this
                .update(cx, |this, cx| {
                    this.workspace.update(cx, |this, cx| {
                        this.project().update(cx, |this, cx| {
                            let worktree_id = worktree.read(cx).id();
                            this.open_buffer(
                                ProjectPath {
                                    worktree_id,
                                    path: relative_path,
                                },
                                cx,
                            )
                        })
                    })
                })??
                .await?;
            let position = buffer.read_with(cx, |this, _| {
                this.snapshot().anchor_after(PointUtf16::new(row, 0))
            });
            let opened_item = this
                .update_in(cx, |this, window, cx| {
                    this.workspace.update(cx, |workspace, cx| {
                        let project_path = buffer
                            .read(cx)
                            .project_path(cx)
                            .context("Could not select a stack frame for unnamed buffer")?;

                        let open_preview = true;

                        let active_debug_line_pane = workspace
                            .project()
                            .read(cx)
                            .breakpoint_store()
                            .read(cx)
                            .active_debug_line_pane_id()
                            .and_then(|id| workspace.pane_for_entity_id(id));

                        let debug_pane = if let Some(pane) = active_debug_line_pane {
                            Some(pane.downgrade())
                        } else {
                            // No debug pane set yet. Find a pane where the target file
                            // is already the active tab so we don't disrupt other panes.
                            let pane_with_active_file = workspace.panes().iter().find(|pane| {
                                pane.read(cx)
                                    .active_item()
                                    .and_then(|item| item.project_path(cx))
                                    .is_some_and(|path| path == project_path)
                            });

                            pane_with_active_file.map(|pane| pane.downgrade())
                        };

                        anyhow::Ok(workspace.open_path_preview_in_tabbed_pane(
                            project_path,
                            debug_pane,
                            true,
                            true,
                            open_preview,
                            window,
                            cx,
                        ))
                    })
                })???
                .await?;

            this.update(cx, |this, cx| {
                let thread_id = this.state.read_with(cx, |state, _| {
                    state.thread_id.context("No selected thread ID found")
                })??;

                this.workspace.update(cx, |workspace, cx| {
                    if let Some(pane_id) = workspace
                        .pane_for(&*opened_item)
                        .map(|pane| pane.entity_id())
                    {
                        workspace
                            .project()
                            .read(cx)
                            .breakpoint_store()
                            .update(cx, |store, _cx| {
                                store.set_active_debug_pane_id(pane_id);
                            });
                    }

                    let breakpoint_store = workspace.project().read(cx).breakpoint_store();

                    breakpoint_store.update(cx, |store, cx| {
                        store.set_active_position(
                            ActiveStackFrame {
                                session_id: this.session.read(cx).session_id(),
                                thread_id,
                                stack_frame_id,
                                path: abs_path,
                                position,
                            },
                            cx,
                        );
                    })
                })
            })?
        })
    }

    pub(crate) fn abs_path_from_stack_frame(stack_frame: &dap::StackFrame) -> Option<Arc<Path>> {
        stack_frame.source.as_ref().and_then(|s| {
            s.path
                .as_deref()
                .filter(|path| {
                    // Since we do not know if we are debugging on the host or (a remote/WSL) target,
                    // we need to check if either the path is absolute as Posix or Windows.
                    is_absolute(path, PathStyle::Posix) || is_absolute(path, PathStyle::Windows)
                })
                .map(|path| Arc::<Path>::from(Path::new(path)))
        })
    }

    pub fn restart_stack_frame(&mut self, stack_frame_id: u64, cx: &mut Context<Self>) {
        self.session.update(cx, |state, cx| {
            state.restart_stack_frame(stack_frame_id, cx)
        });
    }

    pub(crate) fn expand_collapsed_entry(&mut self, ix: usize, cx: &mut Context<Self>) {
        let Some(StackFrameEntry::Collapsed(stack_frames)) = self.entries.get_mut(ix) else {
            return;
        };
        let entries = std::mem::take(stack_frames)
            .into_iter()
            .map(StackFrameEntry::Normal);
        // HERE
        let entries_len = entries.len();
        self.entries.splice(ix..ix + 1, entries);
        let (Ok(filtered_indices_start) | Err(filtered_indices_start)) =
            self.filter_entries_indices.binary_search(&ix);

        for idx in &mut self.filter_entries_indices[filtered_indices_start..] {
            *idx += entries_len - 1;
        }

        self.selected_ix = Some(ix);
        self.list_state.reset(self.entries.len());
        cx.emit(StackFrameListEvent::BuiltEntries);
        cx.notify();
    }

    fn render_collapsed_entry(
        &self,
        ix: usize,
        stack_frames: &Vec<dap::StackFrame>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let first_stack_frame = &stack_frames[0];
        let is_selected = Some(ix) == self.selected_ix;

        h_flex()
            .rounded_md()
            .justify_between()
            .w_full()
            .group("")
            .id(("stack-frame", first_stack_frame.id))
            .p_1()
            .when(is_selected, |this| {
                this.bg(cx.theme().colors().element_hover)
            })
            .on_any_mouse_down(|_, _, cx| {
                cx.stop_propagation();
            })
            .on_click(cx.listener(move |this, _, window, cx| {
                this.selected_ix = Some(ix);
                this.activate_selected_entry(window, cx);
            }))
            .hover(|style| style.bg(cx.theme().colors().element_hover).cursor_pointer())
            .child(
                v_flex()
                    .text_ui_sm(cx)
                    .truncate()
                    .text_color(cx.theme().colors().text_muted)
                    .child(format!(
                        "Show {} more{}",
                        stack_frames.len(),
                        first_stack_frame
                            .source
                            .as_ref()
                            .and_then(|source| source.origin.as_ref())
                            .map_or(String::new(), |origin| format!(": {}", origin))
                    )),
            )
            .into_any()
    }

    fn render_entry(&self, ix: usize, cx: &mut Context<Self>) -> AnyElement {
        let ix = match self.list_filter {
            StackFrameFilter::All => ix,
            StackFrameFilter::OnlyUserFrames => self.filter_entries_indices[ix],
        };

        match &self.entries[ix] {
            StackFrameEntry::Label(stack_frame) => self.render_label_entry(stack_frame, cx),
            StackFrameEntry::Normal(stack_frame) => self.render_normal_entry(ix, stack_frame, cx),
            StackFrameEntry::Collapsed(stack_frames) => {
                self.render_collapsed_entry(ix, stack_frames, cx)
            }
        }
    }

    pub(super) fn select_ix(&mut self, ix: Option<usize>, cx: &mut Context<Self>) {
        self.selected_ix = ix;
        cx.notify();
    }

    pub(super) fn select_next(
        &mut self,
        _: &menu::SelectNext,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ix = match self.selected_ix {
            _ if self.entries.is_empty() => None,
            None => Some(0),
            Some(ix) => {
                if ix == self.entries.len() - 1 {
                    Some(0)
                } else {
                    Some(ix + 1)
                }
            }
        };
        self.select_ix(ix, cx);
    }

    pub(super) fn select_previous(
        &mut self,
        _: &menu::SelectPrevious,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ix = match self.selected_ix {
            _ if self.entries.is_empty() => None,
            None => Some(self.entries.len() - 1),
            Some(ix) => {
                if ix == 0 {
                    Some(self.entries.len() - 1)
                } else {
                    Some(ix - 1)
                }
            }
        };
        self.select_ix(ix, cx);
    }

    pub(super) fn select_first(
        &mut self,
        _: &menu::SelectFirst,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ix = if !self.entries.is_empty() {
            Some(0)
        } else {
            None
        };
        self.select_ix(ix, cx);
    }

    pub(super) fn select_last(
        &mut self,
        _: &menu::SelectLast,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ix = if !self.entries.is_empty() {
            Some(self.entries.len() - 1)
        } else {
            None
        };
        self.select_ix(ix, cx);
    }

    pub(super) fn activate_selected_entry(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ix) = self.selected_ix else {
            return;
        };
        let Some(entry) = self.entries.get_mut(ix) else {
            return;
        };
        match entry {
            StackFrameEntry::Normal(stack_frame) => {
                let stack_frame = stack_frame.clone();
                self.go_to_stack_frame_inner(stack_frame, window, cx)
                    .detach_and_log_err(cx)
            }
            StackFrameEntry::Label(_) => {
                debug_panic!("You should not be able to select a label stack frame")
            }
            StackFrameEntry::Collapsed(_) => self.expand_collapsed_entry(ix, cx),
        }
        cx.notify();
    }

    pub(super) fn confirm(
        &mut self,
        _: &menu::Confirm,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.activate_selected_entry(window, cx);
    }

    pub(crate) fn toggle_frame_filter(
        &mut self,
        thread_status: Option<ThreadStatus>,
        cx: &mut Context<Self>,
    ) {
        self.list_filter = match self.list_filter {
            StackFrameFilter::All => StackFrameFilter::OnlyUserFrames,
            StackFrameFilter::OnlyUserFrames => StackFrameFilter::All,
        };

        if let Some(database_id) = self
            .workspace
            .read_with(cx, |workspace, _| workspace.database_id())
            .ok()
            .flatten()
        {
            let key = stack_frame_filter_key(&self.session.read(cx).adapter(), database_id);
            let kvp = KeyValueStore::global(cx);
            let filter: String = self.list_filter.into();
            cx.background_spawn(async move { kvp.write_kvp(key, filter).await })
                .detach();
        }

        if let Some(ThreadStatus::Stopped) = thread_status {
            match self.list_filter {
                StackFrameFilter::All => {
                    self.list_state.reset(self.entries.len());
                }
                StackFrameFilter::OnlyUserFrames => {
                    self.list_state.reset(self.filter_entries_indices.len());
                    if !self
                        .selected_ix
                        .map(|ix| self.filter_entries_indices.contains(&ix))
                        .unwrap_or_default()
                    {
                        self.selected_ix = None;
                    }
                }
            }

            if let Some(ix) = self.selected_ix {
                let scroll_to = match self.list_filter {
                    StackFrameFilter::All => ix,
                    StackFrameFilter::OnlyUserFrames => self
                        .filter_entries_indices
                        .binary_search_by_key(&ix, |ix| *ix)
                        .expect("This index will always exist"),
                };
                self.list_state.scroll_to_reveal_item(scroll_to);
            }

            cx.emit(StackFrameListEvent::BuiltEntries);
            cx.notify();
        }
    }
}
