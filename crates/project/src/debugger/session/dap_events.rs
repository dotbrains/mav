use super::*;

impl Session {
    fn handle_stopped_event(&mut self, event: StoppedEvent, cx: &mut Context<Self>) {
        self.push_to_history();

        self.state.stopped();
        // todo(debugger): Find a clean way to get around the clone
        let breakpoint_store = self.breakpoint_store.clone();
        if let Some((local, path)) = self.as_running_mut().and_then(|local| {
            let breakpoint = local.tmp_breakpoint.take()?;
            let path = breakpoint.path;
            Some((local, path))
        }) {
            local
                .send_breakpoints_from_path(
                    path,
                    BreakpointUpdatedReason::Toggled,
                    &breakpoint_store,
                    cx,
                )
                .detach();
        };

        if event.all_threads_stopped.unwrap_or_default() || event.thread_id.is_none() {
            self.active_snapshot.thread_states.stop_all_threads();
            self.invalidate_command_type::<StackTraceCommand>();
        }

        // Event if we stopped all threads we still need to insert the thread_id
        // to our own data
        if let Some(thread_id) = event.thread_id {
            self.active_snapshot
                .thread_states
                .stop_thread(ThreadId(thread_id));

            self.invalidate_state(
                &StackTraceCommand {
                    thread_id,
                    start_frame: None,
                    levels: None,
                }
                .into(),
            );
        }

        self.invalidate_generic();
        self.active_snapshot.threads.clear();
        self.active_snapshot.variables.clear();
        cx.emit(SessionEvent::Stopped(
            event
                .thread_id
                .map(Into::into)
                .filter(|_| !event.preserve_focus_hint.unwrap_or(false)),
        ));
        cx.emit(SessionEvent::InvalidateInlineValue);
        cx.notify();
    }

    pub(crate) fn handle_dap_event(&mut self, event: Box<Events>, cx: &mut Context<Self>) {
        match *event {
            Events::Initialized(_) => {
                debug_assert!(
                    false,
                    "Initialized event should have been handled in LocalMode"
                );
            }
            Events::Stopped(event) => self.handle_stopped_event(event, cx),
            Events::Continued(event) => {
                if event.all_threads_continued.unwrap_or_default() {
                    self.active_snapshot.thread_states.continue_all_threads();
                    self.breakpoint_store.update(cx, |store, cx| {
                        store.remove_active_position(Some(self.session_id()), cx)
                    });
                } else {
                    self.active_snapshot
                        .thread_states
                        .continue_thread(ThreadId(event.thread_id));
                }
                // todo(debugger): We should be able to get away with only invalidating generic if all threads were continued
                self.invalidate_generic();
            }
            Events::Exited(_event) => {
                self.clear_active_debug_line(cx);
            }
            Events::Terminated(_) => {
                self.shutdown(cx).detach();
            }
            Events::Thread(event) => {
                let thread_id = ThreadId(event.thread_id);

                match event.reason {
                    dap::ThreadEventReason::Started => {
                        self.active_snapshot
                            .thread_states
                            .continue_thread(thread_id);
                    }
                    dap::ThreadEventReason::Exited => {
                        self.active_snapshot.thread_states.exit_thread(thread_id);
                    }
                    reason => {
                        log::error!("Unhandled thread event reason {:?}", reason);
                    }
                }
                self.invalidate_state(&ThreadsCommand.into());
                cx.notify();
            }
            Events::Output(event) => {
                if event
                    .category
                    .as_ref()
                    .is_some_and(|category| *category == OutputEventCategory::Telemetry)
                {
                    return;
                }

                self.push_output(event);
                cx.notify();
            }
            Events::Breakpoint(event) => self.breakpoint_store.update(cx, |store, _| {
                store.update_session_breakpoint(self.session_id(), event.reason, event.breakpoint);
            }),
            Events::Module(event) => {
                match event.reason {
                    dap::ModuleEventReason::New => {
                        self.active_snapshot.modules.push(event.module);
                    }
                    dap::ModuleEventReason::Changed => {
                        if let Some(module) = self
                            .active_snapshot
                            .modules
                            .iter_mut()
                            .find(|other| event.module.id == other.id)
                        {
                            *module = event.module;
                        }
                    }
                    dap::ModuleEventReason::Removed => {
                        self.active_snapshot
                            .modules
                            .retain(|other| event.module.id != other.id);
                    }
                }

                // todo(debugger): We should only send the invalidate command to downstream clients.
                // self.invalidate_state(&ModulesCommand.into());
            }
            Events::LoadedSource(_) => {
                self.invalidate_state(&LoadedSourcesCommand.into());
            }
            Events::Capabilities(event) => {
                self.capabilities = self.capabilities.merge(event.capabilities);

                // The adapter might've enabled new exception breakpoints (or disabled existing ones).
                let recent_filters = self
                    .capabilities
                    .exception_breakpoint_filters
                    .iter()
                    .flatten()
                    .map(|filter| (filter.filter.clone(), filter.clone()))
                    .collect::<BTreeMap<_, _>>();
                for filter in recent_filters.values() {
                    let default = filter.default.unwrap_or_default();
                    self.exception_breakpoints
                        .entry(filter.filter.clone())
                        .or_insert_with(|| (filter.clone(), default));
                }
                self.exception_breakpoints
                    .retain(|k, _| recent_filters.contains_key(k));
                if self.is_started() {
                    self.send_exception_breakpoints(cx);
                }

                // Remove the ones that no longer exist.
                cx.notify();
            }
            Events::Memory(_) => {}
            Events::Process(_) => {}
            Events::ProgressEnd(_) => {}
            Events::ProgressStart(_) => {}
            Events::ProgressUpdate(_) => {}
            Events::Invalidated(_) => {}
            Events::Other(event) => {
                if event.event == "launchBrowserInCompanion" {
                    let Some(request) = serde_json::from_value(event.body).ok() else {
                        log::error!("failed to deserialize launchBrowserInCompanion event");
                        return;
                    };
                    self.launch_browser_for_remote_server(request, cx);
                } else if event.event == "killCompanionBrowser" {
                    let Some(request) = serde_json::from_value(event.body).ok() else {
                        log::error!("failed to deserialize killCompanionBrowser event");
                        return;
                    };
                    self.kill_browser(request, cx);
                }
            }
        }
    }
}
