use super::*;

impl ToolCallEventStream {
    /// Gate a sandbox *escalation* (network access, per-path writes, or full
    /// filesystem write access) on user approval.
    ///
    /// Offers the user three grant lifetimes — "once", "for the rest of this
    /// thread", and "always". Thread grants live in the shared, in-memory
    /// [`ThreadSandboxGrants`]. Always grants are persisted in agent settings
    /// and are also observed while a prompt is pending, matching the
    /// settings-driven authorization flow for regular tools.
    pub(crate) fn authorize_sandbox(
        &self,
        request: SandboxRequest,
        reason: String,
        cx: &mut App,
    ) -> Task<Result<()>> {
        if Self::sandbox_request_covered_by_grants(&request, &self.sandbox_grants, cx) {
            return Task::ready(Ok(()));
        }

        let (network_hosts, network_all_hosts) = match &request.network {
            crate::sandboxing::NetworkRequest::None => (Vec::new(), false),
            crate::sandboxing::NetworkRequest::AnyHost => (Vec::new(), true),
            crate::sandboxing::NetworkRequest::Hosts(hosts) => {
                (hosts.iter().map(|host| host.to_string()).collect(), false)
            }
        };
        let sandbox_authorization_details = acp_thread::SandboxAuthorizationDetails {
            // The command stays in the tool-call title (set by the terminal
            // tool), so the approval card keeps showing it; the details only
            // describe the requested access and the agent's reason.
            command: None,
            network_hosts,
            network_all_hosts,
            allow_git_access: request.allow_git_access,
            allow_fs_write_all: request.allow_fs_write_all,
            unsandboxed: request.unsandboxed,
            write_paths: request.write_paths.clone(),
            reason,
        };
        let allow_thread_label = if self.is_subagent(cx) {
            "Allow for this subagent"
        } else {
            "Allow for this thread"
        };
        let options = acp_thread::PermissionOptions::Flat(vec![
            acp::PermissionOption::new(
                acp::PermissionOptionId::new(acp_thread::SandboxPermission::AllowOnce.as_id()),
                "Allow once",
                acp::PermissionOptionKind::AllowOnce,
            ),
            acp::PermissionOption::new(
                acp::PermissionOptionId::new(acp_thread::SandboxPermission::AllowThread.as_id()),
                allow_thread_label,
                acp::PermissionOptionKind::AllowAlways,
            ),
            acp::PermissionOption::new(
                acp::PermissionOptionId::new(acp_thread::SandboxPermission::AllowAlways.as_id()),
                "Allow always",
                acp::PermissionOptionKind::AllowAlways,
            ),
            acp::PermissionOption::new(
                acp::PermissionOptionId::new(acp_thread::SandboxPermission::Deny.as_id()),
                "Deny",
                acp::PermissionOptionKind::RejectOnce,
            ),
        ]);

        let fs = self.fs.clone();
        let stream = self.stream.clone();
        let tool_use_id = self.tool_use_id.clone();
        let sandbox_grants = self.sandbox_grants.clone();
        let thread = self.thread.clone();
        let auto_allow_outcome = match auto_resolve_permission_outcome(&options, true) {
            Ok(outcome) => outcome,
            Err(error) => return Task::ready(Err(error)),
        };
        cx.spawn(async move |cx| {
            let (response_tx, mut response_rx) = oneshot::channel();
            if let Err(error) = stream
                .0
                .unbounded_send(Ok(ThreadEvent::ToolCallAuthorization(
                    ToolCallAuthorization {
                        tool_call: acp::ToolCallUpdate::new(
                            tool_use_id.to_string(),
                            // Leave the title untouched so the card keeps
                            // showing the command (matching the fallback flow).
                            acp::ToolCallUpdateFields::new(),
                        )
                        .meta(acp_thread::meta_with_sandbox_authorization(
                            sandbox_authorization_details,
                        )),
                        options,
                        response: response_tx,
                        context: None,
                        kind: acp_thread::AuthorizationKind::PermissionGrant,
                    },
                )))
            {
                log::error!("Failed to send sandbox authorization: {error}");
                return Err(anyhow!("Failed to send sandbox authorization: {error}"));
            }

            let (mut settings_tx, mut settings_rx) = watch::channel(());
            let _settings_subscription = cx.update(|cx| {
                cx.observe_global::<SettingsStore>(move |_cx| {
                    settings_tx.send(()).ok();
                })
            });

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
                        return Self::handle_sandbox_permission_outcome(
                            &outcome,
                            &request,
                            sandbox_grants.clone(),
                            thread.clone(),
                            fs.clone(),
                            cx,
                        );
                    }
                    _ = settings_changed.fuse() => {
                        if cx.update(|cx| Self::sandbox_request_covered_by_grants(
                            &request,
                            &sandbox_grants,
                            cx,
                        )) {
                            drop(response_rx);
                            stream.resolve_tool_call_authorization(
                                &tool_use_id,
                                auto_allow_outcome.clone(),
                            );
                            return Ok(());
                        }
                    }
                }
            }
        })
    }

    fn sandbox_request_covered_by_grants(
        request: &SandboxRequest,
        sandbox_grants: &Rc<RefCell<ThreadSandboxGrants>>,
        cx: &App,
    ) -> bool {
        let settings = AgentSettings::get_global(cx);
        sandbox_grants
            .borrow()
            .covers_with_persistent(request, &settings.sandbox_permissions)
    }

    fn handle_sandbox_permission_outcome(
        outcome: &acp_thread::SelectedPermissionOutcome,
        request: &SandboxRequest,
        sandbox_grants: Rc<RefCell<ThreadSandboxGrants>>,
        thread: Option<WeakEntity<Thread>>,
        fs: Option<Arc<dyn Fs>>,
        cx: &AsyncApp,
    ) -> Result<()> {
        debug_assert!(
            outcome.params.is_none(),
            "unexpected params for sandbox permission"
        );

        match acp_thread::SandboxPermission::from_id(outcome.option_id.0.as_ref()) {
            Some(acp_thread::SandboxPermission::AllowOnce) => Ok(()),
            Some(acp_thread::SandboxPermission::AllowThread) => {
                sandbox_grants.borrow_mut().record(request);
                Self::persist_thread_grants(&thread, cx);
                Ok(())
            }
            Some(acp_thread::SandboxPermission::AllowAlways) => {
                Self::persist_sandbox_always_permission(request, fs, cx);
                Ok(())
            }
            Some(acp_thread::SandboxPermission::Deny) => {
                Err(anyhow!("Permission to run tool denied by user"))
            }
            None => {
                let other = outcome.option_id.0.as_ref();
                debug_assert!(false, "unexpected sandbox permission option_id: {other}");
                Err(anyhow!("Permission to run tool denied by user"))
            }
        }
    }

    fn persist_sandbox_always_permission(
        request: &SandboxRequest,
        fs: Option<Arc<dyn Fs>>,
        cx: &AsyncApp,
    ) {
        let Some(fs) = fs else {
            log::error!(
                "Cannot persist \"allow always\" sandbox permission: no filesystem available"
            );
            return;
        };

        let request = request.clone();
        cx.update(|cx| {
            update_settings_file(fs, cx, move |settings, _| {
                let agent = settings.agent.get_or_insert_default();
                match &request.network {
                    crate::sandboxing::NetworkRequest::None => {}
                    crate::sandboxing::NetworkRequest::AnyHost => {
                        agent.allow_sandbox_all_hosts();
                    }
                    crate::sandboxing::NetworkRequest::Hosts(hosts) => {
                        // Rebuild the persisted list with subsumption pruning
                        // so granting `*.github.com` retires a previously
                        // persisted `api.github.com` instead of accumulating
                        // redundant entries. Unparsable hand-edited entries
                        // are preserved untouched.
                        let mut patterns = Vec::new();
                        let mut unparsable = Vec::new();
                        for raw in agent.sandbox_network_hosts() {
                            match http_proxy::HostPattern::parse(raw) {
                                Ok(pattern) => {
                                    crate::sandboxing::insert_host_pattern(&mut patterns, pattern)
                                }
                                Err(_) => unparsable.push(raw.clone()),
                            }
                        }
                        for host in hosts {
                            crate::sandboxing::insert_host_pattern(&mut patterns, host.clone());
                        }
                        let mut host_strings = unparsable;
                        host_strings.extend(patterns.iter().map(|pattern| pattern.to_string()));
                        agent.set_sandbox_network_hosts(host_strings);
                    }
                }
                if request.allow_git_access {
                    agent.allow_sandbox_git_access();
                }
                if request.allow_fs_write_all {
                    agent.allow_sandbox_fs_write_all();
                }
                if request.unsandboxed {
                    agent.allow_sandbox_unsandboxed();
                }
                for path in request.write_paths {
                    agent.add_sandbox_write_path(path);
                }
            });
        });
    }

    /// The sandbox permissions to actually enforce for a command: the union
    /// of this command's `request`, everything granted "for the rest of the
    /// conversation", and persistent "allow always" sandbox grants.
    ///
    /// Callers must apply this to the enforced sandbox policy (rather than
    /// the raw `request`) so standing grants keep working for later commands
    /// that write to a previously approved path without re-requesting it.
    pub(crate) fn effective_sandbox_request(
        &self,
        request: &SandboxRequest,
        persistent: &agent_settings::SandboxPermissions,
    ) -> SandboxRequest {
        self.sandbox_grants
            .borrow()
            .effective_with_persistent(request, persistent)
    }

    /// Whether the user allowed running commands unsandboxed for the rest of
    /// the thread (distinct from the persistent `allow_unsandboxed` setting).
    pub(crate) fn sandbox_fallback_granted_for_thread(&self) -> bool {
        self.sandbox_grants.borrow().fallback_granted_for_thread()
    }

    /// Whether the user approved a model-requested `unsandboxed: true` escape
    /// for the rest of this thread. Like the fallback grant, this makes every
    /// command in the thread run without a sandbox.
    pub(crate) fn unsandboxed_granted_for_thread(&self) -> bool {
        self.sandbox_grants.borrow().unsandboxed_granted()
    }

    /// Ask the user how to proceed when the OS sandbox could not be created
    /// for a command (for example, `bwrap` is missing or user namespaces are
    /// disabled).
    ///
    /// Unlike [`Self::authorize_sandbox`] — which gates a model-requested
    /// *escalation* — this surfaces a *system limitation*: the sandbox failed,
    /// so the prompt explains why (`reason`) and lets the user retry, run the
    /// command unsandboxed (once / for this thread / always), or deny it. The
    /// "for this thread" choice is recorded in the in-memory thread grants and
    /// "always" is persisted as the `allow_unsandboxed` setting. Only the
    /// Bubblewrap sandboxes (Linux directly, Windows via WSL) can fail to
    /// create a sandbox, so this is gated to those platforms.
    ///
    /// `retries` is how many times the user has already pressed Retry for this
    /// command; it's shown on the button so repeated presses visibly advance
    /// ("Retry", then "Retry (attempt 1)", "Retry (attempt 2)", …).
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    pub(crate) fn authorize_sandbox_fallback(
        &self,
        command: Option<String>,
        reason: String,
        retries: usize,
        cx: &mut App,
    ) -> Task<Result<SandboxFallbackDecision>> {
        let details = acp_thread::SandboxFallbackAuthorizationDetails { command, reason };
        let retry_label = if retries == 0 {
            "Retry".to_string()
        } else {
            format!("Retry (attempt {retries})")
        };
        let allow_thread_label = if self.is_subagent(cx) {
            "Run without sandbox for this subagent"
        } else {
            "Run without sandbox for this thread"
        };
        let options = acp_thread::PermissionOptions::Flat(vec![
            // Retry isn't an allow/deny choice; the UI renders it with its own
            // icon and we dispatch on the option id, so the kind here only
            // governs keybindings. Use `RejectAlways` (which has none) so the
            // "allow once" shortcut maps to "Run without sandbox once" rather
            // than to Retry.
            acp::PermissionOption::new(
                acp::PermissionOptionId::new(acp_thread::SANDBOX_FALLBACK_RETRY_OPTION_ID),
                retry_label,
                acp::PermissionOptionKind::RejectAlways,
            ),
            acp::PermissionOption::new(
                acp::PermissionOptionId::new(acp_thread::SandboxPermission::AllowOnce.as_id()),
                "Run without sandbox once",
                acp::PermissionOptionKind::AllowOnce,
            ),
            acp::PermissionOption::new(
                acp::PermissionOptionId::new(acp_thread::SandboxPermission::AllowThread.as_id()),
                allow_thread_label,
                acp::PermissionOptionKind::AllowAlways,
            ),
            acp::PermissionOption::new(
                acp::PermissionOptionId::new(acp_thread::SandboxPermission::AllowAlways.as_id()),
                "Always run without sandbox",
                acp::PermissionOptionKind::AllowAlways,
            ),
            acp::PermissionOption::new(
                acp::PermissionOptionId::new(acp_thread::SandboxPermission::Deny.as_id()),
                "Deny",
                acp::PermissionOptionKind::RejectOnce,
            ),
        ]);

        let fs = self.fs.clone();
        let stream = self.stream.clone();
        let tool_use_id = self.tool_use_id.clone();
        let sandbox_grants = self.sandbox_grants.clone();
        let thread = self.thread.clone();
        cx.spawn(async move |cx| {
            let (response_tx, response_rx) = oneshot::channel();
            if let Err(error) = stream
                .0
                .unbounded_send(Ok(ThreadEvent::ToolCallAuthorization(
                    ToolCallAuthorization {
                        // Deliberately leave the tool-call title untouched so
                        // the card keeps showing the *command* (not the
                        // failure reason): it's critical the user can see what
                        // they're approving to run unsandboxed. The reason is
                        // surfaced separately by the fallback details / warning.
                        tool_call: acp::ToolCallUpdate::new(
                            tool_use_id.to_string(),
                            acp::ToolCallUpdateFields::new(),
                        )
                        .meta(
                            acp_thread::meta_with_sandbox_fallback_authorization(details),
                        ),
                        options,
                        response: response_tx,
                        context: None,
                        kind: acp_thread::AuthorizationKind::ActionChoice,
                    },
                )))
            {
                log::error!("Failed to send sandbox fallback authorization: {error}");
                return Err(anyhow!(
                    "Failed to send sandbox fallback authorization: {error}"
                ));
            }

            let outcome = response_rx
                .await
                .map_err(|_| anyhow!("authorization channel closed"))?;

            let option_id = outcome.option_id.0.as_ref();
            if option_id == acp_thread::SANDBOX_FALLBACK_RETRY_OPTION_ID {
                return Ok(SandboxFallbackDecision::Retry);
            }
            match acp_thread::SandboxPermission::from_id(option_id) {
                Some(acp_thread::SandboxPermission::AllowOnce) => {
                    Ok(SandboxFallbackDecision::RunUnsandboxed)
                }
                Some(acp_thread::SandboxPermission::AllowThread) => {
                    sandbox_grants.borrow_mut().record_fallback();
                    Self::persist_thread_grants(&thread, cx);
                    Ok(SandboxFallbackDecision::RunUnsandboxed)
                }
                Some(acp_thread::SandboxPermission::AllowAlways) => {
                    sandbox_grants.borrow_mut().record_fallback();
                    Self::persist_thread_grants(&thread, cx);
                    Self::persist_sandbox_unsandboxed_permission(fs, cx);
                    Ok(SandboxFallbackDecision::RunUnsandboxed)
                }
                Some(acp_thread::SandboxPermission::Deny) => Ok(SandboxFallbackDecision::Deny),
                None => {
                    let other = option_id;
                    debug_assert!(false, "unexpected sandbox fallback option_id: {other}");
                    Ok(SandboxFallbackDecision::Deny)
                }
            }
        })
    }

    /// Persist the `allow_unsandboxed` setting. Going forward this turns
    /// sandboxing off for the model-facing surface: later turns expose the
    /// plain `terminal` tool (with no sandbox prompt section) and commands run
    /// without an OS sandbox. On Windows, WSL sandbox setup is skipped.
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    fn persist_sandbox_unsandboxed_permission(fs: Option<Arc<dyn Fs>>, cx: &AsyncApp) {
        let Some(fs) = fs else {
            log::error!(
                "Cannot persist \"allow always\" unsandboxed permission: no filesystem available"
            );
            return;
        };
        cx.update(|cx| {
            update_settings_file(fs, cx, move |settings, _| {
                settings
                    .agent
                    .get_or_insert_default()
                    .allow_sandbox_unsandboxed();
            });
        });
    }
}
