use super::*;

impl MemoryView {
    pub(super) fn go_to_memory_reference(
        &mut self,
        memory_reference: &str,
        evaluate_name: Option<&str>,
        stack_frame_id: Option<u64>,
        cx: &mut Context<Self>,
    ) {
        use parse_int::parse;
        let Ok(as_address) = parse::<u64>(memory_reference) else {
            return;
        };
        let access_size = evaluate_name
            .map(|typ| {
                self.session.update(cx, |this, cx| {
                    this.data_access_size(stack_frame_id, typ, cx)
                })
            })
            .unwrap_or_else(|| Task::ready(None));
        cx.spawn(async move |this, cx| {
            let access_size = access_size.await.unwrap_or(1);
            this.update(cx, |this, cx| {
                this.view_state().selection = Some(SelectedMemoryRange::DragComplete(Drag {
                    start_address: as_address,
                    end_address: as_address + access_size - 1,
                }));
                this.jump_to_address(as_address, cx);
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn handle_memory_drag(&mut self, evt: &DragMoveEvent<Drag>) {
        let mut view_state = self.view_state();
        if !view_state
            .selection
            .as_ref()
            .is_some_and(|selection| selection.is_dragging())
        {
            return;
        }
        let row_count = view_state.row_count();
        debug_assert!(row_count > 1);
        let scroll_handle = &view_state.scroll_handle;
        let viewport = scroll_handle.viewport();

        if viewport.bottom() < evt.event.position.y {
            view_state.schedule_scroll_down();
        } else if viewport.top() > evt.event.position.y {
            view_state.schedule_scroll_up();
        }
    }

    pub(super) fn page_down(
        &mut self,
        _: &menu::SelectLast,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut view_state = self.view_state();
        view_state.base_row = view_state
            .base_row
            .overflowing_add(view_state.row_count())
            .0;
        cx.notify();
    }
    pub(super) fn page_up(
        &mut self,
        _: &menu::SelectFirst,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut view_state = self.view_state();
        view_state.base_row = view_state
            .base_row
            .overflowing_sub(view_state.row_count())
            .0;
        cx.notify();
    }

    pub(super) fn change_query_bar_mode(
        &mut self,
        is_writing_memory: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if is_writing_memory == self.is_writing_memory {
            return;
        }
        if !self.is_writing_memory {
            self.query_editor.update(cx, |this, cx| {
                this.clear(window, cx);
                this.set_placeholder_text("Write to Selected Memory Range", window, cx);
            });
            self.is_writing_memory = true;
            self.query_editor.focus_handle(cx).focus(window, cx);
        } else {
            self.query_editor.update(cx, |this, cx| {
                this.clear(window, cx);
                this.set_placeholder_text("Go to Memory Address / Expression", window, cx);
            });
            self.is_writing_memory = false;
        }
    }

    pub(super) fn toggle_data_breakpoint(
        &mut self,
        _: &crate::ToggleDataBreakpoint,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(SelectedMemoryRange::DragComplete(selection)) =
            self.view_state().selection.clone()
        else {
            return;
        };
        let range = selection.memory_range();
        let context = Arc::new(DataBreakpointContext::Address {
            address: range.start().to_string(),
            bytes: Some(*range.end() - *range.start()),
        });

        self.session.update(cx, |this, cx| {
            let data_breakpoint_info = this.data_breakpoint_info(context.clone(), None, cx);
            cx.spawn(async move |this, cx| {
                if let Some(info) = data_breakpoint_info.await {
                    let Some(data_id) = info.data_id else {
                        return;
                    };
                    _ = this.update(cx, |this, cx| {
                        this.create_data_breakpoint(
                            context,
                            data_id.clone(),
                            dap::DataBreakpoint {
                                data_id,
                                access_type: None,
                                condition: None,
                                hit_condition: None,
                            },
                            cx,
                        );
                    });
                }
            })
            .detach();
        })
    }

    pub(super) fn confirm(
        &mut self,
        _: &menu::Confirm,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let selection = self.view_state().selection.clone();
        if let Some(SelectedMemoryRange::DragComplete(drag)) = selection {
            // Go into memory writing mode.
            if !self.is_writing_memory {
                let should_return = self.session.update(cx, |session, cx| {
                    if !session
                        .capabilities()
                        .supports_write_memory_request
                        .unwrap_or_default()
                    {
                        let adapter_name = session.adapter();
                        // We cannot write memory with this adapter.
                        _ = self.workspace.update(cx, |this, cx| {
                            this.toggle_status_toast(
                                StatusToast::new(format!(
                                    "Debug Adapter `{adapter_name}` does not support writing to memory"
                                ), cx, |this, cx| {
                                    cx.spawn(async move |this, cx| {
                                        cx.background_executor().timer(Duration::from_secs(2)).await;
                                        _ = this.update(cx, |_, cx| {
                                            cx.emit(DismissEvent)
                                        });
                                    }).detach();
                                    this.icon(Icon::new(IconName::XCircle).size(IconSize::Small).color(Color::Error))
                                }),
                                cx,
                            );
                        });
                        true
                    } else {
                        false
                    }
                });
                if should_return {
                    return;
                }

                self.change_query_bar_mode(true, window, cx);
            } else if self.query_editor.focus_handle(cx).is_focused(window) {
                let mut text = self.query_editor.read(cx).text(cx);
                if text.chars().any(|c| !c.is_ascii_hexdigit()) {
                    // Interpret this text as a string and oh-so-conveniently convert it.
                    text = text.bytes().map(|byte| format!("{:02x}", byte)).collect();
                }
                self.session.update(cx, |this, cx| {
                    let range = drag.memory_range();

                    if let Ok(as_hex) = hex::decode(text) {
                        this.write_memory(*range.start(), &as_hex, cx);
                    }
                });
                self.change_query_bar_mode(false, window, cx);
            }

            cx.notify();
            return;
        }
        // Just change the currently viewed address.
        if !self.query_editor.focus_handle(cx).is_focused(window) {
            return;
        }
        self.jump_to_query_bar_address(cx);
    }

    pub(super) fn jump_to_query_bar_address(&mut self, cx: &mut Context<Self>) {
        use parse_int::parse;
        let text = self.query_editor.read(cx).text(cx);

        let Ok(as_address) = parse::<u64>(&text) else {
            return self.jump_to_expression(text, cx);
        };
        self.jump_to_address(as_address, cx);
    }

    pub(super) fn jump_to_address(&mut self, address: u64, cx: &mut Context<Self>) {
        let mut view_state = self.view_state();
        view_state.base_row = (address & !0xfff) / view_state.line_width.width as u64;
        let line_ix = (address & 0xfff) / view_state.line_width.width as u64;
        view_state
            .scroll_handle
            .scroll_to_item(line_ix as usize, ScrollStrategy::Center);
        cx.notify();
    }

    pub(super) fn jump_to_expression(&mut self, expr: String, cx: &mut Context<Self>) {
        let Ok(selected_frame) = self
            .stack_frame_list
            .update(cx, |this, _| this.opened_stack_frame_id())
        else {
            return;
        };
        let expr = format!("?${{{expr}}}");
        let reference = self.session.update(cx, |this, cx| {
            this.memory_reference_of_expr(selected_frame, expr, cx)
        });
        cx.spawn(async move |this, cx| {
            if let Some((reference, typ)) = reference.await {
                _ = this.update(cx, |this, cx| {
                    let sizeof_expr = if typ.as_ref().is_some_and(|t| {
                        t.chars()
                            .all(|c| c.is_whitespace() || c.is_alphabetic() || c == '*')
                    }) {
                        typ.as_deref()
                    } else {
                        None
                    };
                    this.go_to_memory_reference(&reference, sizeof_expr, selected_frame, cx);
                });
            }
        })
        .detach();
    }

    pub(super) fn cancel(&mut self, _: &menu::Cancel, _: &mut Window, cx: &mut Context<Self>) {
        self.view_state().selection = None;
        cx.notify();
    }

    /// Jump to memory pointed to by selected memory range.
    pub(super) fn go_to_address(
        &mut self,
        _: &GoToSelectedAddress,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(SelectedMemoryRange::DragComplete(drag)) = self.view_state().selection.clone()
        else {
            return;
        };
        let range = drag.memory_range();
        let Some(memory): Option<Vec<u8>> = self.session.update(cx, |this, cx| {
            this.read_memory(range, cx).map(|cell| cell.0).collect()
        }) else {
            return;
        };
        if memory.len() > 8 {
            return;
        }
        let zeros_to_write = 8 - memory.len();
        let mut acc = String::from("0x");
        acc.extend(std::iter::repeat("00").take(zeros_to_write));
        let as_query = memory.into_iter().rev().fold(acc, |mut acc, byte| {
            _ = write!(&mut acc, "{:02x}", byte);
            acc
        });
        self.query_editor.update(cx, |this, cx| {
            this.set_text(as_query, window, cx);
        });
        self.jump_to_query_bar_address(cx);
    }

    pub(super) fn deploy_memory_context_menu(
        &mut self,
        range: RangeInclusive<u64>,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let session = self.session.clone();
        let context_menu = ContextMenu::build(window, cx, |menu, _, cx| {
            let range_too_large = range.end() - range.start() > std::mem::size_of::<u64>() as u64;
            let caps = session.read(cx).capabilities();
            let supports_data_breakpoints = caps.supports_data_breakpoints.unwrap_or_default()
                && caps.supports_data_breakpoint_bytes.unwrap_or_default();
            let memory_unreadable = LazyCell::new(|| {
                session.update(cx, |this, cx| {
                    this.read_memory(range.clone(), cx)
                        .any(|cell| cell.0.is_none())
                })
            });

            let mut menu = menu.action_disabled_when(
                range_too_large || *memory_unreadable,
                "Go To Selected Address",
                GoToSelectedAddress.boxed_clone(),
            );

            if supports_data_breakpoints {
                menu = menu.action_disabled_when(
                    *memory_unreadable,
                    "Set Data Breakpoint",
                    ToggleDataBreakpoint { access_type: None }.boxed_clone(),
                );
            }
            menu.context(self.focus_handle.clone())
        });

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

        self.open_context_menu = Some((context_menu, position, subscription));
    }
}
