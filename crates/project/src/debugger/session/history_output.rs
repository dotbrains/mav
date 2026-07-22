use super::*;

impl Session {
    pub fn run_to_position(
        &mut self,
        breakpoint: SourceBreakpoint,
        active_thread_id: ThreadId,
        cx: &mut Context<Self>,
    ) {
        match &mut self.state {
            SessionState::Running(local_mode) => {
                if !matches!(
                    self.active_snapshot
                        .thread_states
                        .thread_state(active_thread_id),
                    Some(ThreadStatus::Stopped)
                ) {
                    return;
                };
                let path = breakpoint.path.clone();
                local_mode.tmp_breakpoint = Some(breakpoint);
                let task = local_mode.send_breakpoints_from_path(
                    path,
                    BreakpointUpdatedReason::Toggled,
                    &self.breakpoint_store,
                    cx,
                );

                cx.spawn(async move |this, cx| {
                    task.await;
                    this.update(cx, |this, cx| {
                        this.continue_thread(active_thread_id, cx);
                    })
                })
                .detach();
            }
            SessionState::Booting(_) => {}
        }
    }

    pub fn has_new_output(&self, last_update: OutputToken) -> bool {
        self.output_token.0.checked_sub(last_update.0).unwrap_or(0) != 0
    }

    pub fn output(
        &self,
        since: OutputToken,
    ) -> (impl Iterator<Item = &dap::OutputEvent>, OutputToken) {
        if self.output_token.0 == 0 {
            return (self.output.range(0..0), OutputToken(0));
        };

        let events_since = self.output_token.0.checked_sub(since.0).unwrap_or(0);

        let clamped_events_since = events_since.clamp(0, self.output.len());
        (
            self.output
                .range(self.output.len() - clamped_events_since..),
            self.output_token,
        )
    }

    pub fn respond_to_client(
        &self,
        request_seq: u64,
        success: bool,
        command: String,
        body: Option<serde_json::Value>,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        let Some(local_session) = self.as_running() else {
            unreachable!("Cannot respond to remote client");
        };
        let client = local_session.client.clone();

        cx.background_spawn(async move {
            client
                .send_message(Message::Response(Response {
                    body,
                    success,
                    command,
                    seq: request_seq + 1,
                    request_seq,
                    message: None,
                }))
                .await
        })
    }

    pub(super) fn session_state(&self) -> &SessionSnapshot {
        self.selected_snapshot_index
            .and_then(|ix| self.snapshots.get(ix))
            .unwrap_or_else(|| &self.active_snapshot)
    }

    pub(super) fn push_to_history(&mut self) {
        if !self.has_ever_stopped() {
            return;
        }

        while self.snapshots.len() >= DEBUG_HISTORY_LIMIT {
            self.snapshots.pop_front();
        }

        self.snapshots
            .push_back(std::mem::take(&mut self.active_snapshot));
    }

    pub fn historic_snapshots(&self) -> &VecDeque<SessionSnapshot> {
        &self.snapshots
    }

    pub fn select_historic_snapshot(&mut self, ix: Option<usize>, cx: &mut Context<Session>) {
        if self.selected_snapshot_index == ix {
            return;
        }

        if self
            .selected_snapshot_index
            .is_some_and(|ix| self.snapshots.len() <= ix)
        {
            debug_panic!("Attempted to select a debug session with an out of bounds index");
            return;
        }

        self.selected_snapshot_index = ix;
        cx.emit(SessionEvent::HistoricSnapshotSelected);
        cx.notify();
    }

    pub fn active_snapshot_index(&self) -> Option<usize> {
        self.selected_snapshot_index
    }

    pub(super) fn push_output(&mut self, event: OutputEvent) {
        self.output.push_back(event);
        self.output_token.0 += 1;
    }
}
