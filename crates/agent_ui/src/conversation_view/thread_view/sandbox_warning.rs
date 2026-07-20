use super::*;

impl ThreadView {
    /// Render the "ran without sandbox" warning shown on a terminal tool card,
    /// tailored to *why* the sandbox wasn't applied.
    pub(super) fn render_sandbox_not_applied_warning(
        &self,
        reason: &SandboxNotAppliedReason,
        cx: &Context<Self>,
    ) -> AnyElement {
        let (title, detail): (SharedString, Option<SharedString>) = match reason {
            SandboxNotAppliedReason::ErrorLinuxWsl(error) => (
                "Couldn't create a sandbox".into(),
                Some(error.user_facing_message().into()),
            ),
            SandboxNotAppliedReason::DisabledForThisThread => {
                let detail = self
                    .find_thread_sandbox_error(cx)
                    .map(|error| {
                        SharedString::from(format!(
                            "Allowed for this thread after the sandbox failed: {}",
                            error.user_facing_message()
                        ))
                    })
                    .unwrap_or_else(|| {
                        "Unsandboxed execution is allowed for the rest of this thread.".into()
                    });
                ("Ran without sandbox".into(), Some(detail))
            }
        };

        h_flex()
            .px_2()
            .py_1()
            .gap_1()
            .border_t_1()
            .border_color(cx.theme().status().warning_border)
            .bg(cx.theme().status().warning_background.opacity(0.5))
            .child(
                h_flex()
                    .min_w_0()
                    .flex_1()
                    .gap_1p5()
                    .items_start()
                    .child(
                        Icon::new(IconName::Warning)
                            .size(IconSize::XSmall)
                            .color(Color::Warning),
                    )
                    .child(
                        v_flex()
                            .min_w_0()
                            .gap_0p5()
                            .child(Label::new(title).size(LabelSize::Small).color(Color::Muted))
                            .when_some(detail, |this, detail| {
                                this.child(
                                    Label::new(detail)
                                        .size(LabelSize::XSmall)
                                        .color(Color::Muted),
                                )
                            }),
                    ),
            )
            .into_any_element()
    }

    /// Find the first terminal tool call in the thread whose sandbox couldn't be
    /// created, so a later "disabled for this thread" warning can reuse the same
    /// explanation of *why* the sandbox failed.
    fn find_thread_sandbox_error(&self, cx: &App) -> Option<acp_thread::LinuxWslSandboxError> {
        self.thread.read(cx).entries().iter().find_map(|entry| {
            if let AgentThreadEntry::ToolCall(tool_call) = entry
                && let Some(SandboxNotAppliedReason::ErrorLinuxWsl(error)) =
                    &tool_call.sandbox_not_applied
            {
                return Some(error.clone());
            }
            None
        })
    }
}
