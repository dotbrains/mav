use super::*;

impl VariableList {
    pub(crate) fn new(
        session: Entity<Session>,
        stack_frame_list: Entity<StackFrameList>,
        memory_view: Entity<MemoryView>,
        weak_running: WeakEntity<RunningState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();

        let _subscriptions = vec![
            cx.subscribe(&stack_frame_list, Self::handle_stack_frame_list_events),
            cx.subscribe(&session, |this, _, event, cx| match event {
                SessionEvent::HistoricSnapshotSelected => {
                    this.selection.take();
                    this.edited_path.take();
                    this.selected_stack_frame_id.take();
                    this.build_entries(cx);
                }
                SessionEvent::Stopped(_) => {
                    this.selection.take();
                    this.edited_path.take();
                    this.selected_stack_frame_id.take();
                }
                SessionEvent::Variables | SessionEvent::Watchers => {
                    this.build_entries(cx);
                }
                _ => {}
            }),
            cx.on_focus_out(&focus_handle, window, |this, _, _, cx| {
                this.edited_path.take();
                cx.notify();
            }),
        ];

        let list_state = UniformListScrollHandle::default();

        Self {
            list_handle: list_state,
            session,
            focus_handle,
            _subscriptions,
            selected_stack_frame_id: None,
            selection: None,
            open_context_menu: None,
            disabled: false,
            edited_path: None,
            entries: Default::default(),
            max_width_index: None,
            entry_states: Default::default(),
            weak_running,
            memory_view,
        }
    }

    pub(super) fn disabled(&mut self, disabled: bool, cx: &mut Context<Self>) {
        let old_disabled = std::mem::take(&mut self.disabled);
        self.disabled = disabled;
        if old_disabled != disabled {
            cx.notify();
        }
    }

    pub(super) fn has_open_context_menu(&self) -> bool {
        self.open_context_menu.is_some()
    }
}
