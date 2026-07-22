use super::*;

impl StackFrameList {
    pub(super) fn schedule_refresh(
        &mut self,
        select_first: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        const REFRESH_DEBOUNCE: Duration = Duration::from_millis(20);

        self._refresh_task = cx.spawn_in(window, async move |this, cx| {
            let debounce = this
                .update(cx, |this, cx| {
                    let new_stack_frames = this.stack_frames(cx);
                    new_stack_frames.unwrap_or_default().is_empty() && !this.entries.is_empty()
                })
                .ok()
                .unwrap_or_default();

            if debounce {
                cx.background_executor().timer(REFRESH_DEBOUNCE).await;
            }
            this.update_in(cx, |this, window, cx| {
                this.build_entries(select_first, window, cx);
            })
            .ok();
        })
    }

    pub(super) fn build_entries(
        &mut self,
        open_first_stack_frame: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let old_selected_frame_id = self
            .selected_ix
            .and_then(|ix| self.entries.get(ix))
            .and_then(|entry| match entry {
                StackFrameEntry::Normal(stack_frame) => Some(stack_frame.id),
                StackFrameEntry::Collapsed(_) | StackFrameEntry::Label(_) => None,
            });
        let mut entries = Vec::new();
        let mut collapsed_entries = Vec::new();
        let mut first_stack_frame = None;
        let mut first_stack_frame_with_path = None;

        let stack_frames = match self.stack_frames(cx) {
            Ok(stack_frames) => stack_frames,
            Err(e) => {
                self.error = Some(format!("{}", e).into());
                self.entries.clear();
                self.selected_ix = None;
                self.list_state.reset(0);
                self.filter_entries_indices.clear();
                cx.emit(StackFrameListEvent::BuiltEntries);
                cx.notify();
                return;
            }
        };

        let worktree_prefixes: Vec<_> = self
            .workspace
            .read_with(cx, |workspace, cx| {
                workspace
                    .visible_worktrees(cx)
                    .map(|tree| tree.read(cx).abs_path())
                    .collect()
            })
            .unwrap_or_default();

        let mut filter_entries_indices = Vec::default();
        for stack_frame in stack_frames.iter() {
            let frame_in_visible_worktree = stack_frame.dap.source.as_ref().is_some_and(|source| {
                source.path.as_ref().is_some_and(|path| {
                    worktree_prefixes
                        .iter()
                        .filter_map(|tree| tree.to_str())
                        .any(|tree| path.starts_with(tree))
                })
            });

            match stack_frame.dap.presentation_hint {
                Some(dap::StackFramePresentationHint::Deemphasize)
                | Some(dap::StackFramePresentationHint::Subtle) => {
                    collapsed_entries.push(stack_frame.dap.clone());
                }
                Some(dap::StackFramePresentationHint::Label) => {
                    entries.push(StackFrameEntry::Label(stack_frame.dap.clone()));
                }
                _ => {
                    let collapsed_entries = std::mem::take(&mut collapsed_entries);
                    if !collapsed_entries.is_empty() {
                        entries.push(StackFrameEntry::Collapsed(collapsed_entries.clone()));
                    }

                    first_stack_frame.get_or_insert(entries.len());

                    if stack_frame
                        .dap
                        .source
                        .as_ref()
                        .is_some_and(|source| source.path.is_some())
                    {
                        first_stack_frame_with_path.get_or_insert(entries.len());
                    }
                    entries.push(StackFrameEntry::Normal(stack_frame.dap.clone()));
                    if frame_in_visible_worktree {
                        filter_entries_indices.push(entries.len() - 1);
                    }
                }
            }
        }

        let collapsed_entries = std::mem::take(&mut collapsed_entries);
        if !collapsed_entries.is_empty() {
            entries.push(StackFrameEntry::Collapsed(collapsed_entries));
        }
        self.entries = entries;
        self.filter_entries_indices = filter_entries_indices;

        if let Some(ix) = first_stack_frame_with_path
            .or(first_stack_frame)
            .filter(|_| open_first_stack_frame)
        {
            self.select_ix(Some(ix), cx);
            self.activate_selected_entry(window, cx);
        } else if let Some(old_selected_frame_id) = old_selected_frame_id {
            let ix = self.entries.iter().position(|entry| match entry {
                StackFrameEntry::Normal(frame) => frame.id == old_selected_frame_id,
                StackFrameEntry::Collapsed(_) | StackFrameEntry::Label(_) => false,
            });
            self.selected_ix = ix;
        }

        match self.list_filter {
            StackFrameFilter::All => {
                self.list_state.reset(self.entries.len());
            }
            StackFrameFilter::OnlyUserFrames => {
                self.list_state.reset(self.filter_entries_indices.len());
            }
        }
        cx.emit(StackFrameListEvent::BuiltEntries);
        cx.notify();
    }
}
