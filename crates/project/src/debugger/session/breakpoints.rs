use super::*;

impl Session {
    pub fn ignore_breakpoints(&self) -> bool {
        self.ignore_breakpoints
    }

    pub fn toggle_ignore_breakpoints(
        &mut self,
        cx: &mut App,
    ) -> Task<HashMap<Arc<Path>, anyhow::Error>> {
        self.set_ignore_breakpoints(!self.ignore_breakpoints, cx)
    }

    pub(crate) fn set_ignore_breakpoints(
        &mut self,
        ignore: bool,
        cx: &mut App,
    ) -> Task<HashMap<Arc<Path>, anyhow::Error>> {
        if self.ignore_breakpoints == ignore {
            return Task::ready(HashMap::default());
        }

        self.ignore_breakpoints = ignore;

        if let Some(local) = self.as_running() {
            local.send_source_breakpoints(ignore, &self.breakpoint_store, cx)
        } else {
            // todo(debugger): We need to propagate this change to downstream sessions and send a message to upstream sessions
            unimplemented!()
        }
    }

    pub fn data_breakpoints(&self) -> impl Iterator<Item = &DataBreakpointState> {
        self.data_breakpoints.values()
    }

    pub fn exception_breakpoints(
        &self,
    ) -> impl Iterator<Item = &(ExceptionBreakpointsFilter, IsEnabled)> {
        self.exception_breakpoints.values()
    }

    pub fn toggle_exception_breakpoint(&mut self, id: &str, cx: &App) {
        if let Some((_, is_enabled)) = self.exception_breakpoints.get_mut(id) {
            *is_enabled = !*is_enabled;
            self.send_exception_breakpoints(cx);
        }
    }

    fn send_exception_breakpoints(&mut self, cx: &App) {
        if let Some(local) = self.as_running() {
            let exception_filters = self
                .exception_breakpoints
                .values()
                .filter_map(|(filter, is_enabled)| is_enabled.then(|| filter.clone()))
                .collect();

            let supports_exception_filters = self
                .capabilities
                .supports_exception_filter_options
                .unwrap_or_default();
            local
                .send_exception_breakpoints(exception_filters, supports_exception_filters)
                .detach_and_log_err(cx);
        } else {
            debug_assert!(false, "Not implemented");
        }
    }

    pub fn toggle_data_breakpoint(&mut self, id: &str, cx: &mut Context<'_, Session>) {
        if let Some(state) = self.data_breakpoints.get_mut(id) {
            state.is_enabled = !state.is_enabled;
            self.send_exception_breakpoints(cx);
        }
    }

    fn send_data_breakpoints(&mut self, cx: &mut Context<Self>) {
        if let Some(mode) = self.as_running() {
            let breakpoints = self
                .data_breakpoints
                .values()
                .filter_map(|state| state.is_enabled.then(|| state.dap.clone()))
                .collect();
            let command = SetDataBreakpointsCommand { breakpoints };
            mode.request(command).detach_and_log_err(cx);
        }
    }

    pub fn create_data_breakpoint(
        &mut self,
        context: Arc<DataBreakpointContext>,
        data_id: String,
        dap: dap::DataBreakpoint,
        cx: &mut Context<Self>,
    ) {
        if self.data_breakpoints.remove(&data_id).is_none() {
            self.data_breakpoints.insert(
                data_id,
                DataBreakpointState {
                    dap,
                    is_enabled: true,
                    context,
                },
            );
        }
        self.send_data_breakpoints(cx);
    }

    pub fn breakpoints_enabled(&self) -> bool {
        self.ignore_breakpoints
    }

    pub fn loaded_sources(&mut self, cx: &mut Context<Self>) -> &[Source] {
        self.fetch(
            dap_command::LoadedSourcesCommand,
            |this, result, cx| {
                let Some(result) = result.log_err() else {
                    return;
                };
                this.active_snapshot.loaded_sources = result;
                cx.emit(SessionEvent::LoadedSources);
                cx.notify();
            },
            cx,
        );
        &self.session_state().loaded_sources
    }
}
