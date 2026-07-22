use super::*;

impl StackFrameList {
    pub fn new(
        workspace: WeakEntity<Workspace>,
        session: Entity<Session>,
        state: WeakEntity<RunningState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();

        let _subscription =
            cx.subscribe_in(&session, window, |this, _, event, window, cx| match event {
                SessionEvent::Threads => {
                    this.schedule_refresh(false, window, cx);
                }
                SessionEvent::Stopped(..)
                | SessionEvent::StackTrace
                | SessionEvent::HistoricSnapshotSelected => {
                    this.schedule_refresh(true, window, cx);
                }
                _ => {}
            });

        let list_state = ListState::new(0, gpui::ListAlignment::Top, px(1000.));

        let list_filter = workspace
            .read_with(cx, |workspace, _| workspace.database_id())
            .ok()
            .flatten()
            .and_then(|database_id| {
                let key = stack_frame_filter_key(&session.read(cx).adapter(), database_id);
                KeyValueStore::global(cx)
                    .read_kvp(&key)
                    .ok()
                    .flatten()
                    .map(StackFrameFilter::from_str_or_default)
            })
            .unwrap_or(StackFrameFilter::All);

        let mut this = Self {
            session,
            workspace,
            focus_handle,
            state,
            _subscription,
            entries: Default::default(),
            filter_entries_indices: Vec::default(),
            error: None,
            selected_ix: None,
            opened_stack_frame_id: None,
            list_filter,
            list_state,
            _refresh_task: Task::ready(()),
        };
        this.schedule_refresh(true, window, cx);
        this
    }

    #[cfg(test)]
    pub(crate) fn entries(&self) -> &Vec<StackFrameEntry> {
        &self.entries
    }

    #[cfg(test)]
    pub(crate) fn flatten_entries(
        &self,
        show_collapsed: bool,
        show_labels: bool,
    ) -> Vec<dap::StackFrame> {
        self.entries
            .iter()
            .enumerate()
            .filter(|(ix, _)| {
                self.list_filter == StackFrameFilter::All
                    || self
                        .filter_entries_indices
                        .binary_search_by_key(&ix, |ix| ix)
                        .is_ok()
            })
            .flat_map(|(_, frame)| match frame {
                StackFrameEntry::Normal(frame) => vec![frame.clone()],
                StackFrameEntry::Label(frame) if show_labels => vec![frame.clone()],
                StackFrameEntry::Collapsed(frames) if show_collapsed => frames.clone(),
                _ => vec![],
            })
            .collect::<Vec<_>>()
    }

    pub(super) fn stack_frames(&self, cx: &mut App) -> Result<Vec<StackFrame>> {
        if let Ok(Some(thread_id)) = self.state.read_with(cx, |state, _| state.thread_id) {
            self.session
                .update(cx, |this, cx| this.stack_frames(thread_id, cx))
        } else {
            Ok(Vec::default())
        }
    }

    #[cfg(test)]
    pub(crate) fn dap_stack_frames(&self, cx: &mut App) -> Vec<dap::StackFrame> {
        match self.list_filter {
            StackFrameFilter::All => self
                .stack_frames(cx)
                .unwrap_or_default()
                .into_iter()
                .map(|stack_frame| stack_frame.dap)
                .collect(),
            StackFrameFilter::OnlyUserFrames => self
                .filter_entries_indices
                .iter()
                .map(|ix| match &self.entries[*ix] {
                    StackFrameEntry::Label(label) => label,
                    StackFrameEntry::Collapsed(_) => panic!("Collapsed tabs should not be visible"),
                    StackFrameEntry::Normal(frame) => frame,
                })
                .cloned()
                .collect(),
        }
    }

    #[cfg(test)]
    pub(crate) fn list_filter(&self) -> StackFrameFilter {
        self.list_filter
    }

    pub fn opened_stack_frame_id(&self) -> Option<StackFrameId> {
        self.opened_stack_frame_id
    }
}
