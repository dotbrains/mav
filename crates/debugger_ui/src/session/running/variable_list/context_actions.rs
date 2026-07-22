use super::*;

impl VariableList {
    pub(super) fn jump_to_variable_memory(
        &mut self,
        _: &GoToMemory,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        _ = maybe!({
            let selection = self.selection.as_ref()?;
            let entry = self.entries.iter().find(|entry| &entry.path == selection)?;
            let var = entry.entry.as_variable()?;
            let memory_reference = var.memory_reference.as_deref()?;

            let sizeof_expr = if var.type_.as_ref().is_some_and(|t| {
                t.chars()
                    .all(|c| c.is_whitespace() || c.is_alphabetic() || c == '*')
            }) {
                var.type_.as_deref()
            } else {
                var.evaluate_name
                    .as_deref()
                    .map(|name| name.strip_prefix("/nat ").unwrap_or_else(|| name))
            };
            self.memory_view.update(cx, |this, cx| {
                this.go_to_memory_reference(
                    memory_reference,
                    sizeof_expr,
                    self.selected_stack_frame_id,
                    cx,
                );
            });
            let weak_panel = self.weak_running.clone();

            window.defer(cx, move |window, cx| {
                _ = weak_panel.update(cx, |this, cx| {
                    this.activate_item(
                        crate::persistence::DebuggerPaneItem::MemoryView,
                        window,
                        cx,
                    );
                });
            });
            Some(())
        });
    }

    pub(super) fn deploy_list_entry_context_menu(
        &mut self,
        entry: ListEntry,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (supports_set_variable, supports_data_breakpoints, supports_go_to_memory) =
            self.session.read_with(cx, |session, _| {
                (
                    session
                        .capabilities()
                        .supports_set_variable
                        .unwrap_or_default(),
                    session
                        .capabilities()
                        .supports_data_breakpoints
                        .unwrap_or_default(),
                    session
                        .capabilities()
                        .supports_read_memory_request
                        .unwrap_or_default(),
                )
            });
        let can_toggle_data_breakpoint = entry
            .as_variable()
            .filter(|_| supports_data_breakpoints)
            .and_then(|variable| {
                let variables_reference = self
                    .entry_states
                    .get(&entry.path)
                    .map(|state| state.parent_reference)?;
                Some(self.session.update(cx, |session, cx| {
                    session.data_breakpoint_info(
                        Arc::new(DataBreakpointContext::Variable {
                            variables_reference,
                            name: variable.name.clone(),
                            bytes: None,
                        }),
                        None,
                        cx,
                    )
                }))
            });

        let focus_handle = self.focus_handle.clone();
        cx.spawn_in(window, async move |this, cx| {
            let can_toggle_data_breakpoint = if let Some(task) = can_toggle_data_breakpoint {
                task.await
            } else {
                None
            };
            cx.update(|window, cx| {
                let context_menu = ContextMenu::build(window, cx, |menu, _, _| {
                    menu.when_some(entry.as_variable(), |menu, _| {
                        menu.action("Copy Name", CopyVariableName.boxed_clone())
                            .action("Copy Value", CopyVariableValue.boxed_clone())
                            .when(supports_set_variable, |menu| {
                                menu.action("Edit Value", EditVariable.boxed_clone())
                            })
                            .when(supports_go_to_memory, |menu| {
                                menu.action("Go To Memory", GoToMemory.boxed_clone())
                            })
                            .action("Watch Variable", AddWatch.boxed_clone())
                            .when_some(can_toggle_data_breakpoint, |mut menu, data_info| {
                                menu = menu.separator();
                                if let Some(access_types) = data_info.access_types {
                                    for access in access_types {
                                        menu = menu.action(
                                            format!(
                                                "Toggle {} Data Breakpoint",
                                                match access {
                                                    dap::DataBreakpointAccessType::Read => "Read",
                                                    dap::DataBreakpointAccessType::Write => "Write",
                                                    dap::DataBreakpointAccessType::ReadWrite =>
                                                        "Read/Write",
                                                }
                                            ),
                                            crate::ToggleDataBreakpoint {
                                                access_type: Some(access),
                                            }
                                            .boxed_clone(),
                                        );
                                    }

                                    menu
                                } else {
                                    menu.action(
                                        "Toggle Data Breakpoint",
                                        crate::ToggleDataBreakpoint { access_type: None }
                                            .boxed_clone(),
                                    )
                                }
                            })
                    })
                    .when(entry.as_watcher().is_some(), |menu| {
                        menu.action("Copy Name", CopyVariableName.boxed_clone())
                            .action("Copy Value", CopyVariableValue.boxed_clone())
                            .when(supports_set_variable, |menu| {
                                menu.action("Edit Value", EditVariable.boxed_clone())
                            })
                            .action("Remove Watch", RemoveWatch.boxed_clone())
                    })
                    .context(focus_handle.clone())
                });

                _ = this.update(cx, |this, cx| {
                    cx.focus_view(&context_menu, window);
                    let subscription = cx.subscribe_in(
                        &context_menu,
                        window,
                        |this, _, _: &DismissEvent, window, cx| {
                            if this.open_context_menu.as_ref().is_some_and(|context_menu| {
                                context_menu.0.focus_handle(cx).contains_focused(window, cx)
                            }) {
                                cx.focus_self(window);
                            }
                            this.open_context_menu.take();
                            cx.notify();
                        },
                    );

                    this.open_context_menu = Some((context_menu, position, subscription));
                });
            })
        })
        .detach();
    }

    pub(super) fn toggle_data_breakpoint(
        &mut self,
        data_info: &crate::ToggleDataBreakpoint,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(entry) = self
            .selection
            .as_ref()
            .and_then(|selection| self.entries.iter().find(|entry| &entry.path == selection))
        else {
            return;
        };

        let Some((name, var_ref)) = entry.as_variable().map(|var| &var.name).zip(
            self.entry_states
                .get(&entry.path)
                .map(|state| state.parent_reference),
        ) else {
            return;
        };

        let context = Arc::new(DataBreakpointContext::Variable {
            variables_reference: var_ref,
            name: name.clone(),
            bytes: None,
        });
        let data_breakpoint = self.session.update(cx, |session, cx| {
            session.data_breakpoint_info(context.clone(), None, cx)
        });

        let session = self.session.downgrade();
        let access_type = data_info.access_type;
        cx.spawn(async move |_, cx| {
            let Some((data_id, access_types)) = data_breakpoint
                .await
                .and_then(|info| Some((info.data_id?, info.access_types)))
            else {
                return;
            };

            // Because user's can manually add this action to the keymap
            // we check if access type is supported
            let access_type = match access_types {
                None => None,
                Some(access_types) => {
                    if access_type.is_some_and(|access_type| access_types.contains(&access_type)) {
                        access_type
                    } else {
                        None
                    }
                }
            };
            _ = session.update(cx, |session, cx| {
                session.create_data_breakpoint(
                    context,
                    data_id.clone(),
                    dap::DataBreakpoint {
                        data_id,
                        access_type,
                        condition: None,
                        hit_condition: None,
                    },
                    cx,
                );
                cx.notify();
            });
        })
        .detach();
    }
}
