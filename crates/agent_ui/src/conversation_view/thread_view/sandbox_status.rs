use super::*;

impl ThreadView {
    fn refresh_sandbox_status(&mut self, cx: &mut Context<Self>) -> Option<VerifiedSandboxStatus> {
        let thread = self.as_native_thread(cx)?;
        let (key, refresh) =
            thread.update(cx, |thread, cx| thread.refresh_verified_sandbox_status(cx))?;

        if self.sandbox_status_key.as_ref() == Some(&key) {
            return self.sandbox_status.clone();
        }

        match refresh {
            SandboxStatusRefresh::Ready(status) => {
                self.sandbox_status = Some(status.clone());
                self.sandbox_status_key = Some(key);
                self.pending_sandbox_status_key = None;
                Some(status)
            }
            SandboxStatusRefresh::Pending(task) => {
                if self.pending_sandbox_status_key.as_ref() != Some(&key) {
                    self.sandbox_status = None;
                    self.sandbox_status_key = None;
                    self.pending_sandbox_status_key = Some(key.clone());
                    self._sandbox_status_refresh_task = Some(cx.spawn(async move |this, cx| {
                        let status = task.await;
                        this.update(cx, |this, cx| {
                            if this.pending_sandbox_status_key.as_ref() == Some(&key) {
                                this.sandbox_status = Some(status);
                                this.sandbox_status_key = Some(key);
                                this.pending_sandbox_status_key = None;
                                cx.notify();
                            }
                        })
                        .ok();
                    }));
                }
                None
            }
        }
    }

    pub fn render_sandbox_status(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let status = self.refresh_sandbox_status(cx)?;
        let settings_sandbox = status.settings_sandbox.clone();
        let thread_sandbox = status.thread_sandbox.clone();
        let baseline = status.baseline_writable_paths;

        // The lock is struck only when the *merged* result is unsandboxed (the
        // agent runs with ambient permissions). A layer that is merely wide open
        // but still sandboxed keeps the closed lock.
        let (icon, icon_color) = if settings_sandbox
            .clone()
            .merge(thread_sandbox.clone())
            .is_unsandboxed()
        {
            (IconName::LockOff, Color::Muted)
        } else {
            (IconName::Lock, Color::Default)
        };

        let tooltip = match (settings_sandbox, thread_sandbox) {
            // No sandbox at all because the user turned it off in settings: the
            // per-thread layer is moot, so don't show it.
            (ThreadSandbox::Unsandboxed, _) => SandboxStatusTooltip::disabled_in_settings(),
            // Sandboxed by settings, but disabled for this thread: show the
            // settings scope (greyed) for context above the disabled status.
            (ThreadSandbox::Sandboxed(settings_policy), ThreadSandbox::Unsandboxed) => {
                let settings = augment_settings_sandbox_policy(settings_policy, baseline);
                SandboxStatusTooltip::disabled_for_thread(sandbox_section(
                    "Defined in your settings:",
                    &settings,
                    true,
                ))
            }
            (
                ThreadSandbox::Sandboxed(settings_policy),
                ThreadSandbox::Sandboxed(thread_policy),
            ) => {
                let settings = augment_settings_sandbox_policy(settings_policy, baseline);
                // Omit the per-thread section when it grants nothing extra.
                let thread = (!sandbox_policy_grants_nothing(&thread_policy))
                    .then(|| sandbox_section("Allowed for this thread:", &thread_policy, false));
                SandboxStatusTooltip::enabled(
                    sandbox_section("Defined in your settings:", &settings, true),
                    thread,
                )
            }
        };

        Some(
            h_flex()
                .gap_1()
                .child(
                    IconButton::new("sandbox-status", icon)
                        .icon_size(IconSize::Small)
                        .icon_color(icon_color)
                        .tooltip(Tooltip::element(move |_window, _cx| {
                            tooltip.clone().into_any_element()
                        }))
                        .on_click(|_, window, cx| {
                            window.dispatch_action(
                                Box::new(mav_actions::OpenSettingsAt {
                                    path: mav_actions::AGENT_SANDBOX_SETTINGS_PATH.to_string(),
                                    target: None,
                                }),
                                cx,
                            );
                        }),
                )
                .child(Divider::vertical().h_4())
                .into_any_element(),
        )
    }
}
