use super::*;

impl BreakpointList {
    pub(super) fn render_list(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let selected_ix = self.selected_ix;
        let focus_handle = self.focus_handle.clone();
        let supported_breakpoint_properties = self
            .session
            .as_ref()
            .map(|session| SupportedBreakpointProperties::from(session.read(cx).capabilities()))
            .unwrap_or_else(SupportedBreakpointProperties::all);
        let strip_mode = self.strip_mode;

        uniform_list(
            "breakpoint-list",
            self.breakpoints.len(),
            cx.processor(move |this, range: Range<usize>, _, _| {
                range
                    .clone()
                    .zip(&mut this.breakpoints[range])
                    .map(|(ix, breakpoint)| {
                        breakpoint
                            .render(
                                strip_mode,
                                supported_breakpoint_properties,
                                ix,
                                Some(ix) == selected_ix,
                                focus_handle.clone(),
                            )
                            .into_any_element()
                    })
                    .collect()
            }),
        )
        .with_horizontal_sizing_behavior(gpui::ListHorizontalSizingBehavior::Unconstrained)
        .with_width_from_item(self.max_width_index)
        .track_scroll(&self.scroll_handle)
        .flex_1()
    }

    pub(crate) fn render_control_strip(&self) -> AnyElement {
        let selection_kind = self.selection_kind();
        let focus_handle = self.focus_handle.clone();

        let remove_breakpoint_tooltip = selection_kind.map(|(kind, _)| match kind {
            SelectedBreakpointKind::Source => "Remove breakpoint from a breakpoint list",
            SelectedBreakpointKind::Exception => {
                "Exception Breakpoints cannot be removed from the breakpoint list"
            }
            SelectedBreakpointKind::Data => "Remove data breakpoint from a breakpoint list",
        });

        let toggle_label = selection_kind.map(|(_, is_enabled)| {
            if is_enabled {
                (
                    "Disable Breakpoint",
                    "Disable a breakpoint without removing it from the list",
                )
            } else {
                ("Enable Breakpoint", "Re-enable a breakpoint")
            }
        });

        h_flex()
            .child(
                IconButton::new(
                    "disable-breakpoint-breakpoint-list",
                    IconName::DebugDisabledBreakpoint,
                )
                .icon_size(IconSize::Small)
                .when_some(toggle_label, |this, (label, meta)| {
                    this.tooltip({
                        let focus_handle = focus_handle.clone();
                        move |_window, cx| {
                            Tooltip::with_meta_in(
                                label,
                                Some(&ToggleEnableBreakpoint),
                                meta,
                                &focus_handle,
                                cx,
                            )
                        }
                    })
                })
                .disabled(selection_kind.is_none())
                .on_click({
                    let focus_handle = focus_handle.clone();
                    move |_, window, cx| {
                        focus_handle.focus(window, cx);
                        window.dispatch_action(ToggleEnableBreakpoint.boxed_clone(), cx)
                    }
                }),
            )
            .child(
                IconButton::new("remove-breakpoint-breakpoint-list", IconName::Trash)
                    .icon_size(IconSize::Small)
                    .when_some(remove_breakpoint_tooltip, |this, tooltip| {
                        this.tooltip({
                            let focus_handle = focus_handle.clone();
                            move |_window, cx| {
                                Tooltip::with_meta_in(
                                    "Remove Breakpoint",
                                    Some(&UnsetBreakpoint),
                                    tooltip,
                                    &focus_handle,
                                    cx,
                                )
                            }
                        })
                    })
                    .disabled(
                        selection_kind.map(|kind| kind.0) != Some(SelectedBreakpointKind::Source),
                    )
                    .on_click({
                        move |_, window, cx| {
                            focus_handle.focus(window, cx);
                            window.dispatch_action(UnsetBreakpoint.boxed_clone(), cx)
                        }
                    }),
            )
            .into_any_element()
    }
}
