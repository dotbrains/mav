use super::*;

impl ConversationView {
    pub(super) fn emit_load_error_telemetry(&self, error: &LoadError) {
        let error_kind = match error {
            LoadError::Unsupported { .. } => "unsupported",
            LoadError::FailedToInstall(_) => "failed_to_install",
            LoadError::Exited { .. } => "exited",
            LoadError::Other(_) => "other",
        };

        let agent_name = self.agent.agent_id();

        telemetry::event!(
            "Agent Panel Error Shown",
            agent = agent_name,
            kind = error_kind,
            message = error.to_string(),
        );
    }

    pub(super) fn render_load_error(
        &self,
        e: &LoadError,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (title, message, action_slot): (_, SharedString, _) = match e {
            LoadError::Unsupported {
                command: path,
                current_version,
                minimum_version,
            } => {
                return self.render_unsupported(path, current_version, minimum_version, window, cx);
            }
            LoadError::FailedToInstall(msg) => (
                "Failed to Install",
                msg.into(),
                Some(self.create_copy_button(msg.to_string()).into_any_element()),
            ),
            LoadError::Exited { status, stderr } => {
                let mut message = format!("Server exited with status {status}");
                if let Some(stderr) = stderr {
                    message.push_str("\n");
                    message.push_str(stderr);
                };
                let action_slot = stderr
                    .is_some()
                    .then(|| self.create_copy_button(message.clone()).into_any_element());
                ("Failed to Launch", message.into(), action_slot)
            }
            LoadError::Other(msg) => (
                "Failed to Launch",
                msg.into(),
                Some(self.create_copy_button(msg.to_string()).into_any_element()),
            ),
        };

        Callout::new()
            .severity(Severity::Error)
            .icon(IconName::XCircleFilled)
            .title(title)
            .description(message)
            .actions_slot(div().children(action_slot))
            .into_any_element()
    }

    fn render_unsupported(
        &self,
        path: &SharedString,
        version: &SharedString,
        minimum_version: &SharedString,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (heading_label, description_label) = (
            format!("Upgrade {} to work with Mav", self.agent.agent_id()),
            if version.is_empty() {
                format!(
                    "Currently using {}, which does not report a valid --version",
                    path,
                )
            } else {
                format!(
                    "Currently using {}, which is only version {} (need at least {minimum_version})",
                    path, version
                )
            },
        );

        v_flex()
            .w_full()
            .p_3p5()
            .gap_2p5()
            .border_t_1()
            .border_color(cx.theme().colors().border)
            .bg(linear_gradient(
                180.,
                linear_color_stop(cx.theme().colors().editor_background.opacity(0.4), 4.),
                linear_color_stop(cx.theme().status().info_background.opacity(0.), 0.),
            ))
            .child(
                v_flex().gap_0p5().child(Label::new(heading_label)).child(
                    Label::new(description_label)
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                ),
            )
            .into_any_element()
    }
}
