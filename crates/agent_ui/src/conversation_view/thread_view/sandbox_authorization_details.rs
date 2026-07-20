use super::*;

impl ThreadView {
    pub(super) fn render_sandbox_authorization_details(
        &self,
        entry_ix: usize,
        tool_call_id: &acp::ToolCallId,
        details: &SandboxAuthorizationDetails,
        cx: &Context<Self>,
    ) -> AnyElement {
        let has_network = details.network_all_hosts || !details.network_hosts.is_empty();
        let has_write = details.allow_fs_write_all || !details.write_paths.is_empty();
        if !has_network
            && !has_write
            && !details.allow_git_access
            && !details.unsandboxed
            && details.reason.is_empty()
        {
            return Empty.into_any_element();
        }

        let network_section = has_network.then(|| {
            let summary = if details.network_all_hosts {
                "any host".to_string()
            } else {
                format!(
                    "{} {}",
                    details.network_hosts.len(),
                    if details.network_hosts.len() == 1 {
                        "host"
                    } else {
                        "hosts"
                    }
                )
            };
            let has_host_list = !details.network_all_hosts && !details.network_hosts.is_empty();
            let is_open = !self
                .collapsed_sandbox_network_details
                .contains(tool_call_id);
            let mut hosts = details.network_hosts.clone();
            hosts.sort();

            v_flex()
                .child(
                    h_flex()
                        .id(("sandbox-network-details-header", entry_ix))
                        .px_2()
                        .py_1()
                        .justify_between()
                        .when(has_host_list, |this| {
                            this.cursor_pointer()
                                .hover(|style| style.bg(cx.theme().colors().element_hover))
                                .on_click(cx.listener({
                                    let tool_call_id = tool_call_id.clone();
                                    move |this, _event, _window, cx| {
                                        if this
                                            .collapsed_sandbox_network_details
                                            .remove(&tool_call_id)
                                        {
                                            cx.notify();
                                            return;
                                        }

                                        this.collapsed_sandbox_network_details
                                            .insert(tool_call_id.clone());
                                        cx.notify();
                                    }
                                }))
                        })
                        .child(
                            h_flex()
                                .gap_1()
                                .child(
                                    Label::new("Network access")
                                        .size(LabelSize::Small)
                                        .color(Color::Muted),
                                )
                                .child(
                                    Label::new("•")
                                        .size(LabelSize::XSmall)
                                        .color(Color::Disabled),
                                )
                                .child(
                                    Label::new(summary)
                                        .size(LabelSize::Small)
                                        .color(Color::Muted),
                                ),
                        )
                        .when(has_host_list, |this| {
                            this.child(
                                Disclosure::new(("sandbox-network-details", entry_ix), is_open)
                                    .opened_icon(IconName::ChevronUp)
                                    .closed_icon(IconName::ChevronDown),
                            )
                        }),
                )
                .when(has_host_list && is_open, |this| {
                    this.child(
                        v_flex()
                            .id(("sandbox-network-hosts-list", entry_ix))
                            .max_h_40()
                            .overflow_y_scroll()
                            .children(hosts.iter().enumerate().map(|(host_ix, host)| {
                                h_flex()
                                    .min_w_0()
                                    .px_2()
                                    .py_1p5()
                                    .bg(cx.theme().colors().editor_background)
                                    .when(host_ix < hosts.len() - 1, |this| {
                                        this.border_b_1().border_color(cx.theme().colors().border)
                                    })
                                    .child(
                                        Label::new(host.clone())
                                            .size(LabelSize::XSmall)
                                            .buffer_font(cx),
                                    )
                            })),
                    )
                })
        });

        let write_section = has_write.then(|| {
            let summary = if details.allow_fs_write_all {
                "unrestricted".to_string()
            } else {
                format!(
                    "{} {}",
                    details.write_paths.len(),
                    if details.write_paths.len() == 1 {
                        "path"
                    } else {
                        "paths"
                    }
                )
            };
            let has_path_list = !details.allow_fs_write_all && !details.write_paths.is_empty();
            let is_open = !self
                .collapsed_sandbox_authorization_details
                .contains(tool_call_id);
            let mut paths = details.write_paths.clone();
            paths.sort();

            v_flex()
                .child(
                    h_flex()
                        .id(("sandbox-authorization-details-header", entry_ix))
                        .px_2()
                        .py_1()
                        .justify_between()
                        .when(has_path_list, |this| {
                            this.cursor_pointer()
                                .hover(|style| style.bg(cx.theme().colors().element_hover))
                                .on_click(cx.listener({
                                    let tool_call_id = tool_call_id.clone();
                                    move |this, _event, _window, cx| {
                                        if this
                                            .collapsed_sandbox_authorization_details
                                            .remove(&tool_call_id)
                                        {
                                            cx.notify();
                                            return;
                                        }

                                        this.collapsed_sandbox_authorization_details
                                            .insert(tool_call_id.clone());
                                        cx.notify();
                                    }
                                }))
                        })
                        .child(
                            h_flex()
                                .gap_1()
                                .child(
                                    Label::new("Write access")
                                        .size(LabelSize::Small)
                                        .color(Color::Muted),
                                )
                                .child(
                                    Label::new("•")
                                        .size(LabelSize::XSmall)
                                        .color(Color::Disabled),
                                )
                                .child(
                                    Label::new(summary)
                                        .size(LabelSize::Small)
                                        .color(Color::Muted),
                                ),
                        )
                        .when(has_path_list, |this| {
                            this.child(
                                Disclosure::new(
                                    ("sandbox-authorization-details", entry_ix),
                                    is_open,
                                )
                                .opened_icon(IconName::ChevronUp)
                                .closed_icon(IconName::ChevronDown),
                            )
                        }),
                )
                .when(has_path_list && is_open, |this| {
                    this.child(
                        v_flex()
                            .id(("sandbox-authorization-paths-list", entry_ix))
                            .max_h_40()
                            .overflow_y_scroll()
                            .children(paths.iter().enumerate().map(|(path_ix, path)| {
                                self.render_sandbox_authorization_path_row(
                                    entry_ix,
                                    path_ix,
                                    path,
                                    path_ix < paths.len() - 1,
                                    cx,
                                )
                            })),
                    )
                })
        });

        let unsandboxed_section = details.unsandboxed.then(|| {
            h_flex()
                .px_2()
                .py_1()
                .gap_1p5()
                .child(
                    Icon::new(IconName::Warning)
                        .color(Color::Warning)
                        .size(IconSize::Small),
                )
                .child(
                    Label::new("Runs without the OS sandbox")
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                )
        });

        let git_access_section = details.allow_git_access.then(|| {
            v_flex().px_2().py_1().gap_0p5().child(
                h_flex()
                    .gap_1()
                    .child(
                        Icon::new(IconName::GitBranch)
                            .color(Color::Muted)
                            .size(IconSize::Small),
                    )
                    .child(
                        Label::new("Git metadata access")
                            .size(LabelSize::Small)
                            .color(Color::Muted),
                    ),
            )
        });

        let reason_section = (!details.reason.is_empty()).then(|| {
            v_flex()
                .px_2()
                .py_1()
                .gap_0p5()
                .child(
                    Label::new("Reason from agent")
                        .size(LabelSize::XSmall)
                        .color(Color::Muted)
                        .buffer_font(cx),
                )
                .child(Label::new(details.reason.clone()).size(LabelSize::Small))
        });

        v_flex()
            .border_t_1()
            .border_color(self.tool_card_border_color(cx))
            .children(network_section)
            .children(write_section)
            .children(git_access_section)
            .children(unsandboxed_section)
            .children(reason_section)
            .into_any_element()
    }

    pub(super) fn render_sandbox_fallback_authorization_details(
        &self,
        details: &SandboxFallbackAuthorizationDetails,
        cx: &Context<Self>,
    ) -> AnyElement {
        if details.reason.is_empty() {
            return Empty.into_any_element();
        }

        h_flex()
            .p_1p5()
            .gap_1p5()
            .items_start()
            .border_t_1()
            .border_color(self.tool_card_border_color(cx))
            .child(
                Icon::new(IconName::Warning)
                    .color(Color::Warning)
                    .size(IconSize::Small),
            )
            .child(
                v_flex()
                    .min_w_0()
                    .gap_0p5()
                    .child(
                        Label::new("Couldn't create a sandbox")
                            .size(LabelSize::Small)
                            .color(Color::Muted),
                    )
                    .child(Label::new(details.reason.clone()).size(LabelSize::Small)),
            )
            .into_any_element()
    }

    fn render_sandbox_authorization_path_row(
        &self,
        entry_ix: usize,
        path_ix: usize,
        path: &Path,
        show_border: bool,
        cx: &Context<Self>,
    ) -> Stateful<Div> {
        let display_path = path.display().to_string();
        let file_name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| display_path.clone());
        let parent_path = path.parent().and_then(|parent| {
            let parent = parent.display().to_string();
            (!parent.is_empty()).then_some(parent)
        });

        h_flex()
            .id(SharedString::from(format!(
                "sandbox-authorization-path-{entry_ix}-{path_ix}"
            )))
            .min_w_0()
            .px_2()
            .py_1p5()
            .bg(cx.theme().colors().editor_background)
            .when(show_border, |this| {
                this.border_b_1().border_color(cx.theme().colors().border)
            })
            .child(
                h_flex()
                    .id(SharedString::from(format!(
                        "sandbox-authorization-path-name-{entry_ix}-{path_ix}"
                    )))
                    .min_w_0()
                    .gap_0p5()
                    .child(
                        Label::new(file_name)
                            .size(LabelSize::XSmall)
                            .buffer_font(cx),
                    )
                    .when_some(parent_path, |this, parent_path| {
                        this.child(
                            Label::new(format!(" {parent_path}"))
                                .color(Color::Muted)
                                .size(LabelSize::XSmall)
                                .buffer_font(cx),
                        )
                    })
                    .tooltip(move |_window, cx| {
                        Tooltip::with_meta("Requested write path", None, display_path.clone(), cx)
                    }),
            )
    }
}
