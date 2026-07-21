use super::*;

impl ToolCallEventStream {
    /// Authorize a third-party tool (e.g., MCP tool from a context server).
    ///
    /// Unlike built-in tools, third-party tools don't support pattern-based permissions.
    /// They only support `default` (allow/deny/confirm) per tool.
    ///
    /// Uses the dropdown authorization flow with two granularities:
    /// - "Always for <display_name> MCP tool" → sets `tools.<tool_id>.default = "allow"` or "deny"
    /// - "Only this time" → allow/deny once
    pub fn authorize_third_party_tool(
        &self,
        title: impl Into<String>,
        tool_id: String,
        display_name: String,
        cx: &mut App,
    ) -> Task<Result<()>> {
        let title = title.into();
        let options = acp_thread::PermissionOptions::Dropdown(vec![
            acp_thread::PermissionOptionChoice {
                allow: acp::PermissionOption::new(
                    acp::PermissionOptionId::new(format!("always_allow_mcp:{tool_id}")),
                    format!("Always for {display_name} MCP tool"),
                    acp::PermissionOptionKind::AllowAlways,
                ),
                deny: acp::PermissionOption::new(
                    acp::PermissionOptionId::new(format!("always_deny_mcp:{tool_id}")),
                    format!("Always for {display_name} MCP tool"),
                    acp::PermissionOptionKind::RejectAlways,
                ),
                sub_patterns: vec![],
            },
            acp_thread::PermissionOptionChoice {
                allow: acp::PermissionOption::new(
                    acp::PermissionOptionId::new("allow"),
                    "Only this time",
                    acp::PermissionOptionKind::AllowOnce,
                ),
                deny: acp::PermissionOption::new(
                    acp::PermissionOptionId::new("deny"),
                    "Only this time",
                    acp::PermissionOptionKind::RejectOnce,
                ),
                sub_patterns: vec![],
            },
        ]);

        // MCP tools are gated only by tool id (no per-input pattern
        // matching), so we pass a single empty input value just to satisfy
        // `decide_permission_from_settings`' signature.
        let check_settings: Box<dyn Fn(&App) -> ToolPermissionDecision> =
            Box::new(move |cx: &App| {
                let settings = agent_settings::AgentSettings::get_global(cx);
                decide_permission_from_settings(&tool_id, &[String::new()], settings)
            });

        self.run_authorization_loop(title, options, None, Some(check_settings), cx)
    }

    /// Gate a tool call on user permission, driven by the agent's
    /// tool-permission settings.
    ///
    /// Evaluates the current settings up-front: returns `Ok(())` immediately
    /// if the tool is already allowed, an error if it is denied, and
    /// otherwise prompts the user for a decision. While a prompt is pending,
    /// a subscription to `SettingsStore` watches for changes (for example,
    /// when the user clicks "Always for …" on a sibling tool call and the
    /// new rule becomes globally visible). When settings change, the current
    /// prompt is dismissed and the decision is re-evaluated. This closes the
    /// gap where an "Always for …" decision on one pending tool call would
    /// not propagate to other pending tool calls in the same turn or in
    /// subagent turns.
    ///
    /// For authorizations that must always prompt regardless of settings
    /// (e.g. symlink-escape confirmations, sensitive settings-file edits),
    /// use [`Self::prompt`] instead.
    pub fn authorize(
        &self,
        title: impl Into<String>,
        context: ToolPermissionContext,
        cx: &mut App,
    ) -> Task<Result<()>> {
        let title = title.into();
        let options = context.build_permission_options();

        let tool_name = context.tool_name.clone();
        let input_values = context.input_values.clone();
        let check_settings: Box<dyn Fn(&App) -> ToolPermissionDecision> =
            Box::new(move |cx: &App| {
                decide_permission_from_settings(
                    &tool_name,
                    &input_values,
                    agent_settings::AgentSettings::get_global(cx),
                )
            });

        self.run_authorization_loop(title, options, Some(context), Some(check_settings), cx)
    }

    /// Like [`Self::authorize`], but always prompts the user without
    /// consulting settings. Use this for authorizations that must be
    /// confirmed even when the user has configured `always_allow` rules —
    /// for example, symlink-escape confirmations or edits that target
    /// sensitive settings files.
    pub fn authorize_always_prompt(
        &self,
        title: impl Into<String>,
        context: ToolPermissionContext,
        cx: &mut App,
    ) -> Task<Result<()>> {
        let title = title.into();
        let options = context.build_permission_options();
        self.run_authorization_loop(title, options, Some(context), None, cx)
    }

    /// Prompts the user for authorization.
    ///
    /// When `check_settings` is `Some`, this gate is settings-driven: the
    /// settings are evaluated up-front (an Allow or Deny result resolves the
    /// task immediately without prompting), and while a prompt is pending a
    /// `SettingsStore` subscription watches for changes. A subsequent Allow
    /// or Deny dismisses the prompt UI and resolves the task without user
    /// interaction.
    ///
    /// When `check_settings` is `None`, the user is always prompted and
    /// settings changes are ignored. This suits prompts that aren't
    /// settings-driven (e.g. symlink-escape confirmations).
    fn run_authorization_loop(
        &self,
        title: String,
        options: acp_thread::PermissionOptions,
        context: Option<ToolPermissionContext>,
        check_settings: Option<Box<dyn Fn(&App) -> ToolPermissionDecision>>,
        cx: &mut App,
    ) -> Task<Result<()>> {
        // Short-circuit when current settings yield a definitive answer.
        if let Some(check) = check_settings.as_ref() {
            match check(cx) {
                ToolPermissionDecision::Allow => return Task::ready(Ok(())),
                ToolPermissionDecision::Deny(reason) => {
                    return Task::ready(Err(anyhow!(reason)));
                }
                ToolPermissionDecision::Confirm => {}
            }
        }

        let fs = self.fs.clone();
        let stream = self.stream.clone();
        let tool_use_id = self.tool_use_id.clone();
        let auto_resolution_outcomes = if check_settings.is_some() {
            match (
                auto_resolve_permission_outcome(&options, true),
                auto_resolve_permission_outcome(&options, false),
            ) {
                (Ok(allow), Ok(deny)) => Some((allow, deny)),
                (Err(error), _) | (_, Err(error)) => return Task::ready(Err(error)),
            }
        } else {
            None
        };
        cx.spawn(async move |cx| {
            let (response_tx, mut response_rx) = oneshot::channel();
            if let Err(error) = stream
                .0
                .unbounded_send(Ok(ThreadEvent::ToolCallAuthorization(
                    ToolCallAuthorization {
                        tool_call: acp::ToolCallUpdate::new(
                            tool_use_id.to_string(),
                            acp::ToolCallUpdateFields::new().title(title),
                        ),
                        options,
                        response: response_tx,
                        context,
                        kind: acp_thread::AuthorizationKind::PermissionGrant,
                    },
                )))
            {
                log::error!("Failed to send tool call authorization: {error}");
                return Err(anyhow!("Failed to send tool call authorization: {error}"));
            }

            let Some(check_settings) = check_settings else {
                let outcome = response_rx
                    .await
                    .map_err(|_| anyhow!("authorization channel closed"))?;

                return Self::persist_permission_outcome(&outcome, fs, cx);
            };
            let Some((auto_allow_outcome, auto_deny_outcome)) = auto_resolution_outcomes else {
                return Err(anyhow!("missing auto-resolution outcomes"));
            };

            let (mut settings_tx, mut settings_rx) = watch::channel(());
            let _settings_subscription = cx.update(|cx| {
                cx.observe_global::<SettingsStore>(move |_cx| {
                    settings_tx.send(()).ok();
                })
            });

            // Race the user's response against settings changes. On each
            // settings change, re-evaluate `check_settings`: if it now
            // yields a definitive Allow or Deny, resolve the prompt
            // without user interaction. Otherwise keep waiting on the
            // same prompt.
            loop {
                let settings_changed = async {
                    if settings_rx.changed().await.is_err() {
                        std::future::pending::<()>().await;
                    }
                };
                futures::select_biased! {
                    outcome = (&mut response_rx).fuse() => {
                        let outcome = outcome
                            .map_err(|_| anyhow!("authorization channel closed"))?;
                        return Self::persist_permission_outcome(&outcome, fs.clone(), cx);
                    }
                    _ = settings_changed.fuse() => {
                        // On auto-resolve, we dismiss the prompt UI by
                        // resolving the tool call's `WaitingForConfirmation`
                        // status with an internal selected outcome. Dropping
                        // `response_rx` prevents the synthetic response from
                        // being delivered back into this loop.
                        match cx.update(|cx| check_settings(cx)) {
                            ToolPermissionDecision::Allow => {
                                drop(response_rx);
                                stream.resolve_tool_call_authorization(
                                    &tool_use_id,
                                    auto_allow_outcome.clone(),
                                );
                                return Ok(());
                            }
                            ToolPermissionDecision::Deny(reason) => {
                                drop(response_rx);
                                stream.resolve_tool_call_authorization(
                                    &tool_use_id,
                                    auto_deny_outcome.clone(),
                                );
                                return Err(anyhow!(reason));
                            }
                            ToolPermissionDecision::Confirm => continue,
                        }
                    }
                }
            }
        })
    }

    /// Interprets a `SelectedPermissionOutcome` and persists any settings changes.
    /// Returns `true` if the tool call should be allowed, `false` if denied.
    fn persist_permission_outcome(
        outcome: &acp_thread::SelectedPermissionOutcome,
        fs: Option<Arc<dyn Fs>>,
        cx: &AsyncApp,
    ) -> Result<()> {
        let option_id = outcome.option_id.0.as_ref();
        let err = || Err(anyhow!("Permission to run tool denied by user"));

        let always_permission = option_id
            .strip_prefix("always_allow:")
            .map(|tool| (tool, ToolPermissionMode::Allow))
            .or_else(|| {
                option_id
                    .strip_prefix("always_deny:")
                    .map(|tool| (tool, ToolPermissionMode::Deny))
            })
            .or_else(|| {
                option_id
                    .strip_prefix("always_allow_mcp:")
                    .map(|tool| (tool, ToolPermissionMode::Allow))
            })
            .or_else(|| {
                option_id
                    .strip_prefix("always_deny_mcp:")
                    .map(|tool| (tool, ToolPermissionMode::Deny))
            });

        if let Some((tool, mode)) = always_permission {
            let params = outcome.params.as_ref();
            Self::persist_always_permission(tool, mode, params, fs, cx);
            return if mode == ToolPermissionMode::Allow {
                Ok(())
            } else {
                err()
            };
        }

        // Handle simple "allow" / "deny" (once, no persistence)
        if option_id == "allow" || option_id == "deny" {
            debug_assert!(
                outcome.params.is_none(),
                "unexpected params for once-only permission"
            );
            return if option_id == "allow" { Ok(()) } else { err() };
        }

        debug_assert!(false, "unexpected permission option_id: {option_id}");

        err()
    }

    /// Persists an "always allow" or "always deny" permission, using sub_patterns
    /// from params when present.
    fn persist_always_permission(
        tool: &str,
        mode: ToolPermissionMode,
        params: Option<&acp_thread::SelectedPermissionParams>,
        fs: Option<Arc<dyn Fs>>,
        cx: &AsyncApp,
    ) {
        let Some(fs) = fs else {
            return;
        };

        match params {
            Some(acp_thread::SelectedPermissionParams::Terminal {
                patterns: sub_patterns,
            }) => {
                debug_assert!(
                    !sub_patterns.is_empty(),
                    "empty sub_patterns for tool {tool} — callers should pass None instead"
                );
                let tool = tool.to_string();
                let sub_patterns = sub_patterns.clone();
                cx.update(|cx| {
                    update_settings_file(fs, cx, move |settings, _| {
                        let agent = settings.agent.get_or_insert_default();
                        for pattern in sub_patterns {
                            match mode {
                                ToolPermissionMode::Allow => {
                                    agent.add_tool_allow_pattern(&tool, pattern);
                                }
                                ToolPermissionMode::Deny => {
                                    agent.add_tool_deny_pattern(&tool, pattern);
                                }
                                // If there's no matching pattern this will
                                // default to confirm, so falling through is
                                // fine here.
                                ToolPermissionMode::Confirm => (),
                            }
                        }
                    });
                });
            }
            None => {
                let tool = tool.to_string();
                cx.update(|cx| {
                    update_settings_file(fs, cx, move |settings, _| {
                        settings
                            .agent
                            .get_or_insert_default()
                            .set_tool_default_permission(&tool, mode);
                    });
                });
            }
        }
    }
}
