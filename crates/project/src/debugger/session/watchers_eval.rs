use super::*;

impl Session {
    pub fn watchers(&self) -> &HashMap<SharedString, Watcher> {
        &self.watchers
    }

    pub fn add_watcher(
        &mut self,
        expression: SharedString,
        frame_id: u64,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        let request = self.state.request_dap(EvaluateCommand {
            expression: expression.to_string(),
            context: Some(EvaluateArgumentsContext::Watch),
            frame_id: Some(frame_id),
            source: None,
        });

        cx.spawn(async move |this, cx| {
            let response = request.await?;

            this.update(cx, |session, cx| {
                session.watchers.insert(
                    expression.clone(),
                    Watcher {
                        expression,
                        value: response.result.into(),
                        variables_reference: response.variables_reference,
                        presentation_hint: response.presentation_hint,
                    },
                );
                cx.emit(SessionEvent::Watchers);
            })
        })
    }

    pub fn refresh_watchers(&mut self, frame_id: u64, cx: &mut Context<Self>) {
        let watches = self.watchers.clone();
        for (_, watch) in watches.into_iter() {
            self.add_watcher(watch.expression.clone(), frame_id, cx)
                .detach();
        }
    }

    pub fn remove_watcher(&mut self, expression: SharedString) {
        self.watchers.remove(&expression);
    }

    pub fn variables(
        &mut self,
        variables_reference: VariableReference,
        cx: &mut Context<Self>,
    ) -> Vec<dap::Variable> {
        let command = VariablesCommand {
            variables_reference,
            filter: None,
            start: None,
            count: None,
            format: None,
        };

        self.fetch(
            command,
            move |this, variables, cx| {
                let Some(mut variables) = variables.log_err() else {
                    return;
                };

                if this.adapter.0.as_ref() == "Debugpy" {
                    for variable in variables.iter_mut() {
                        if variable.type_ == Some("str".into()) {
                            // reverse Python repr() escaping
                            let mut unescaped = String::with_capacity(variable.value.len());
                            let mut chars = variable.value.chars();
                            while let Some(c) = chars.next() {
                                if c != '\\' {
                                    unescaped.push(c);
                                } else {
                                    match chars.next() {
                                        Some('\\') => unescaped.push('\\'),
                                        Some('n') => unescaped.push('\n'),
                                        Some('t') => unescaped.push('\t'),
                                        Some('r') => unescaped.push('\r'),
                                        Some('\'') => unescaped.push('\''),
                                        Some('"') => unescaped.push('"'),
                                        Some(c) => {
                                            unescaped.push('\\');
                                            unescaped.push(c);
                                        }
                                        None => {}
                                    }
                                }
                            }
                            variable.value = unescaped;
                        }
                    }
                }

                this.active_snapshot
                    .variables
                    .insert(variables_reference, variables);

                cx.emit(SessionEvent::Variables);
                cx.emit(SessionEvent::InvalidateInlineValue);
            },
            cx,
        );

        self.session_state()
            .variables
            .get(&variables_reference)
            .cloned()
            .unwrap_or_default()
    }

    pub fn data_breakpoint_info(
        &mut self,
        context: Arc<DataBreakpointContext>,
        mode: Option<String>,
        cx: &mut Context<Self>,
    ) -> Task<Option<dap::DataBreakpointInfoResponse>> {
        let command = DataBreakpointInfoCommand { context, mode };

        self.request(command, |_, response, _| response.ok(), cx)
    }

    pub fn set_variable_value(
        &mut self,
        stack_frame_id: u64,
        variables_reference: u64,
        name: String,
        value: String,
        cx: &mut Context<Self>,
    ) {
        if self.capabilities.supports_set_variable.unwrap_or_default() {
            self.request(
                SetVariableValueCommand {
                    name,
                    value,
                    variables_reference,
                },
                move |this, response, cx| {
                    let response = response.log_err()?;
                    this.invalidate_command_type::<VariablesCommand>();
                    this.invalidate_command_type::<ReadMemory>();
                    this.memory.clear(cx.background_executor());
                    this.refresh_watchers(stack_frame_id, cx);
                    cx.emit(SessionEvent::Variables);
                    Some(response)
                },
                cx,
            )
            .detach();
        }
    }

    pub fn evaluate(
        &mut self,
        expression: String,
        context: Option<EvaluateArgumentsContext>,
        frame_id: Option<u64>,
        source: Option<Source>,
        cx: &mut Context<Self>,
    ) -> Task<()> {
        let event = dap::OutputEvent {
            category: None,
            output: format!("> {expression}"),
            group: None,
            variables_reference: None,
            source: None,
            line: None,
            column: None,
            data: None,
            location_reference: None,
        };
        self.push_output(event);
        let request = self.state.request_dap(EvaluateCommand {
            expression,
            context,
            frame_id,
            source,
        });
        cx.spawn(async move |this, cx| {
            let response = request.await;
            this.update(cx, |this, cx| {
                this.memory.clear(cx.background_executor());
                this.invalidate_command_type::<ReadMemory>();
                this.invalidate_command_type::<VariablesCommand>();
                cx.emit(SessionEvent::Variables);
                match response {
                    Ok(response) => {
                        let event = dap::OutputEvent {
                            category: None,
                            output: format!("< {}", &response.result),
                            group: None,
                            variables_reference: Some(response.variables_reference),
                            source: None,
                            line: None,
                            column: None,
                            data: None,
                            location_reference: None,
                        };
                        this.push_output(event);
                    }
                    Err(e) => {
                        let event = dap::OutputEvent {
                            category: None,
                            output: format!("{}", e),
                            group: None,
                            variables_reference: None,
                            source: None,
                            line: None,
                            column: None,
                            data: None,
                            location_reference: None,
                        };
                        this.push_output(event);
                    }
                };
                cx.notify();
            })
            .ok();
        })
    }
}
