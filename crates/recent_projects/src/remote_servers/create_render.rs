use super::*;

impl RemoteServerProjects {
    fn render_create_remote_server(
        &self,
        state: &CreateRemoteServer,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let ssh_prompt = state.ssh_prompt.clone();

        state.address_editor.update(cx, |editor, cx| {
            if editor.text(cx).is_empty() {
                editor.set_placeholder_text("ssh user@example -p 2222", window, cx);
            }
        });

        let theme = cx.theme();

        v_flex()
            .track_focus(&self.focus_handle(cx))
            .id("create-remote-server")
            .overflow_hidden()
            .size_full()
            .flex_1()
            .child(
                div()
                    .p_2()
                    .border_b_1()
                    .border_color(theme.colors().border_variant)
                    .child(state.address_editor.clone()),
            )
            .child(
                h_flex()
                    .bg(theme.colors().editor_background)
                    .rounded_b_sm()
                    .w_full()
                    .map(|this| {
                        if let Some(ssh_prompt) = ssh_prompt {
                            this.child(h_flex().w_full().child(ssh_prompt))
                        } else if let Some(address_error) = &state.address_error {
                            this.child(
                                h_flex().p_2().w_full().gap_2().child(
                                    Label::new(address_error.clone())
                                        .size(LabelSize::Small)
                                        .color(Color::Error),
                                ),
                            )
                        } else {
                            this.child(
                                h_flex()
                                    .p_2()
                                    .w_full()
                                    .gap_1()
                                    .child(
                                        Label::new(
                                            "Enter the command you use to SSH into this server.",
                                        )
                                        .color(Color::Muted)
                                        .size(LabelSize::Small),
                                    )
                                    .child(
                                        Button::new("learn-more", "Learn More")
                                            .label_size(LabelSize::Small)
                                            .end_icon(
                                                Icon::new(IconName::ArrowUpRight)
                                                    .size(IconSize::XSmall),
                                            )
                                            .on_click(|_, _, cx| {
                                                cx.open_url(
                                                    "https://mav.dev/docs/remote-development",
                                                );
                                            }),
                                    ),
                            )
                        }
                    }),
            )
    }

    #[cfg(target_os = "windows")]
    fn render_add_wsl_distro(
        &self,
        state: &AddWslDistro,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let connection_prompt = state.connection_prompt.clone();

        state.picker.update(cx, |picker, cx| {
            picker.focus_handle(cx).focus(window, cx);
        });

        v_flex()
            .id("add-wsl-distro")
            .overflow_hidden()
            .size_full()
            .flex_1()
            .map(|this| {
                if let Some(connection_prompt) = connection_prompt {
                    this.child(connection_prompt)
                } else {
                    this.child(state.picker.clone())
                }
            })
    }
}
