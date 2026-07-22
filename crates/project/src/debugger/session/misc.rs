use super::*;

impl Session {
    pub fn location(
        &mut self,
        reference: u64,
        cx: &mut Context<Self>,
    ) -> Option<dap::LocationsResponse> {
        self.fetch(
            LocationsCommand { reference },
            move |this, response, _| {
                let Some(response) = response.log_err() else {
                    return;
                };
                this.active_snapshot.locations.insert(reference, response);
            },
            cx,
        );
        self.session_state().locations.get(&reference).cloned()
    }

    pub fn is_attached(&self) -> bool {
        let SessionState::Running(local_mode) = &self.state else {
            return false;
        };
        local_mode.binary.request_args.request == StartDebuggingRequestArgumentsRequest::Attach
    }

    pub fn disconnect_client(&mut self, cx: &mut Context<Self>) {
        let command = DisconnectCommand {
            restart: Some(false),
            terminate_debuggee: Some(false),
            suspend_debuggee: Some(false),
        };

        self.request(command, Self::empty_response, cx).detach()
    }

    pub fn terminate_threads(&mut self, thread_ids: Option<Vec<ThreadId>>, cx: &mut Context<Self>) {
        if self
            .capabilities
            .supports_terminate_threads_request
            .unwrap_or_default()
        {
            self.request(
                TerminateThreadsCommand {
                    thread_ids: thread_ids.map(|ids| ids.into_iter().map(|id| id.0).collect()),
                },
                Self::clear_active_debug_line_response,
                cx,
            )
            .detach();
        } else {
            self.shutdown(cx).detach();
        }
    }

    pub fn thread_state(&self, thread_id: ThreadId) -> Option<ThreadStatus> {
        self.session_state().thread_states.thread_state(thread_id)
    }

    pub fn quirks(&self) -> SessionQuirks {
        self.quirks
    }
}
