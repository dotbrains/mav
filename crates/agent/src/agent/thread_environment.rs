use super::*;

pub struct NativeThreadEnvironment {
    pub(super) agent: WeakEntity<NativeAgent>,
    pub(super) thread: WeakEntity<Thread>,
    pub(super) acp_thread: WeakEntity<AcpThread>,
}

impl NativeThreadEnvironment {
    pub(crate) fn create_subagent_thread(
        &self,
        label: String,
        cx: &mut App,
    ) -> Result<Rc<dyn SubagentHandle>> {
        let Some(parent_thread_entity) = self.thread.upgrade() else {
            anyhow::bail!("Parent thread no longer exists".to_string());
        };
        let parent_thread = parent_thread_entity.read(cx);
        let current_depth = parent_thread.depth();
        let parent_session_id = parent_thread.id().clone();

        if current_depth >= MAX_SUBAGENT_DEPTH {
            return Err(anyhow!(
                "Maximum subagent depth ({}) reached",
                MAX_SUBAGENT_DEPTH
            ));
        }

        let subagent_thread: Entity<Thread> = cx.new(|cx| {
            let mut thread = Thread::new_subagent(&parent_thread_entity, cx);
            thread.set_title(label.into(), cx);
            thread
        });

        let session_id = subagent_thread.read(cx).id().clone();

        let acp_thread = self
            .agent
            .update(cx, |agent, cx| -> Result<Entity<AcpThread>> {
                let project_id = agent
                    .sessions
                    .get(&parent_session_id)
                    .map(|s| s.project_id)
                    .context("parent session not found")?;
                Ok(agent.register_session(subagent_thread.clone(), project_id, 1, cx))
            })??;

        let depth = current_depth + 1;

        telemetry::event!(
            "Subagent Started",
            session = parent_thread_entity.read(cx).id().to_string(),
            subagent_session = session_id.to_string(),
            depth,
            is_resumed = false,
        );

        self.prompt_subagent(session_id, subagent_thread, acp_thread)
    }

    pub(crate) fn resume_subagent_thread(
        &self,
        session_id: acp::SessionId,
        cx: &mut App,
    ) -> Result<Rc<dyn SubagentHandle>> {
        let (subagent_thread, acp_thread) = self.agent.update(cx, |agent, _cx| {
            let session = agent
                .sessions
                .get(&session_id)
                .ok_or_else(|| anyhow!("No subagent session found with id {session_id}"))?;
            anyhow::Ok((session.thread.clone(), session.acp_thread.clone()))
        })??;

        let depth = subagent_thread.read(cx).depth();

        if let Some(parent_thread_entity) = self.thread.upgrade() {
            telemetry::event!(
                "Subagent Started",
                session = parent_thread_entity.read(cx).id().to_string(),
                subagent_session = session_id.to_string(),
                depth,
                is_resumed = true,
            );
        }

        self.prompt_subagent(session_id, subagent_thread, acp_thread)
    }

    fn prompt_subagent(
        &self,
        session_id: acp::SessionId,
        subagent_thread: Entity<Thread>,
        acp_thread: Entity<acp_thread::AcpThread>,
    ) -> Result<Rc<dyn SubagentHandle>> {
        let Some(parent_thread_entity) = self.thread.upgrade() else {
            anyhow::bail!("Parent thread no longer exists".to_string());
        };
        Ok(Rc::new(NativeSubagentHandle::new(
            session_id,
            subagent_thread,
            acp_thread,
            parent_thread_entity,
        )) as _)
    }
}

impl ThreadEnvironment for NativeThreadEnvironment {
    fn create_terminal(
        &self,
        command: String,
        extra_env: Vec<acp::EnvVariable>,
        cwd: Option<PathBuf>,
        output_byte_limit: Option<u64>,
        sandbox_wrap: Option<acp_thread::SandboxWrap>,
        cx: &mut AsyncApp,
    ) -> Task<Result<Rc<dyn TerminalHandle>>> {
        // On Seatbelt-style sandboxes (macOS) there's no tmpfs overlay, so to
        // give the command a writable temp area we point `$TMPDIR`/`$TMP`/
        // `$TEMP` at a per-thread directory inside the sandbox's writable
        // scope. Doing this even when sandboxing is disabled keeps `$TMPDIR`
        // stable so the model can't infer sandbox state from it.
        //
        // Only do this for local projects. For remote projects the temp
        // directory would be created on the client, but the terminal runs on
        // the remote host, so pointing `$TMPDIR` (and the sandbox writable
        // scope) at a client-side path would leak client environment into the
        // remote terminal and reference a directory that doesn't exist there.
        //
        // Linux and Windows are excluded: the bwrap sandbox (run directly on
        // Linux, and via WSL on Windows) already mounts a fresh, writable
        // `tmpfs` over `/tmp`, so the environment looks like a normal
        // filesystem with no special `$TMPDIR` (which would only make the
        // sandbox more obviously Mav-specific). On Windows a per-thread
        // `$TMPDIR` would also be a Windows path that's meaningless inside
        // WSL, and adding it to the writable scope would bind a stray
        // `/mnt/<drive>/...` path.
        #[cfg_attr(any(target_os = "linux", target_os = "windows"), allow(unused_mut))]
        let mut extra_env = extra_env;
        #[cfg_attr(any(target_os = "linux", target_os = "windows"), allow(unused_mut))]
        let mut sandbox_wrap = sandbox_wrap;
        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        {
            let temp_dir = self.thread.update(cx, |thread, cx| {
                thread
                    .project()
                    .read(cx)
                    .is_local()
                    .then(|| thread.sandboxed_terminal_temp_dir(cx))
            });
            match temp_dir {
                Ok(Some(Ok(temp_dir))) => {
                    // Canonicalize so the path matches what the sandbox
                    // resolves symlinks to (e.g. `/var` -> `/private/var` on
                    // macOS). `$TMPDIR` and the writable-scope entry below must
                    // agree, and they must agree with the path the kernel
                    // actually checks.
                    let temp_dir = temp_dir.canonicalize().unwrap_or(temp_dir);
                    let temp_dir_string = temp_dir.to_string_lossy().into_owned();
                    extra_env.extend([
                        acp::EnvVariable::new("TMPDIR", &temp_dir_string),
                        acp::EnvVariable::new("TMP", &temp_dir_string),
                        acp::EnvVariable::new("TEMP", &temp_dir_string),
                    ]);
                    // The command's `$TMPDIR` must live inside the sandbox's
                    // writable scope. The per-thread temp directory is owned
                    // here (not in the terminal tool that assembles the rest
                    // of the writable set), so add it whenever the command is
                    // sandboxed.
                    if let Some(sandbox_wrap) = &mut sandbox_wrap {
                        sandbox_wrap.writable_paths.push(temp_dir);
                    }
                }
                Ok(None) => {}
                Ok(Some(Err(error))) => return Task::ready(Err(error)),
                Err(error) => return Task::ready(Err(error)),
            };
        }
        let task = self.acp_thread.update(cx, |thread, cx| {
            thread.create_terminal(
                command,
                vec![],
                extra_env,
                cwd,
                output_byte_limit,
                sandbox_wrap,
                cx,
            )
        });

        let acp_thread = self.acp_thread.clone();
        cx.spawn(async move |cx| {
            let terminal = task?.await?;

            let (drop_tx, drop_rx) = oneshot::channel();
            let terminal_id = terminal.read_with(cx, |terminal, _cx| terminal.id().clone());

            cx.spawn(async move |cx| {
                drop_rx.await.ok();
                acp_thread.update(cx, |thread, cx| thread.release_terminal(terminal_id, cx))
            })
            .detach();

            let handle = AcpTerminalHandle {
                terminal,
                _drop_tx: Some(drop_tx),
            };

            Ok(Rc::new(handle) as _)
        })
    }

    fn create_subagent(&self, label: String, cx: &mut App) -> Result<Rc<dyn SubagentHandle>> {
        self.create_subagent_thread(label, cx)
    }

    fn resume_subagent(
        &self,
        session_id: acp::SessionId,
        cx: &mut App,
    ) -> Result<Rc<dyn SubagentHandle>> {
        self.resume_subagent_thread(session_id, cx)
    }

    fn create_sibling_thread(
        &self,
        request: SiblingThreadRequest,
        cx: &mut AsyncApp,
    ) -> Task<Result<SiblingThreadInfo>> {
        let host = match self
            .agent
            .read_with(cx, |agent, _| agent.sibling_thread_host())
        {
            Ok(Some(host)) => host,
            Ok(None) => {
                return Task::ready(Err(anyhow!(
                    "No sibling-thread host is registered. This usually means the \
                     agent panel hasn't been initialized in this workspace."
                )));
            }
            Err(err) => return Task::ready(Err(err)),
        };
        host.create_sibling_thread(request, cx)
    }

    fn list_available_agents(&self, cx: &mut App) -> Result<AvailableAgents> {
        let host = self
            .agent
            .read_with(cx, |agent, _| agent.sibling_thread_host())?
            .ok_or_else(|| {
                anyhow!(
                    "No sibling-thread host is registered. This usually means the \
                     agent panel hasn't been initialized in this workspace."
                )
            })?;
        host.list_available_agents(cx)
    }
}

#[derive(Debug, Clone)]
enum SubagentPromptResult {
    Completed,
    Cancelled,
    ContextWindowWarning,
    Error(String),
}

pub struct NativeSubagentHandle {
    session_id: acp::SessionId,
    parent_thread: WeakEntity<Thread>,
    subagent_thread: Entity<Thread>,
    acp_thread: Entity<acp_thread::AcpThread>,
}

impl NativeSubagentHandle {
    fn new(
        session_id: acp::SessionId,
        subagent_thread: Entity<Thread>,
        acp_thread: Entity<acp_thread::AcpThread>,
        parent_thread_entity: Entity<Thread>,
    ) -> Self {
        NativeSubagentHandle {
            session_id,
            subagent_thread,
            parent_thread: parent_thread_entity.downgrade(),
            acp_thread,
        }
    }
}

impl SubagentHandle for NativeSubagentHandle {
    fn id(&self) -> acp::SessionId {
        self.session_id.clone()
    }

    fn num_entries(&self, cx: &App) -> usize {
        self.acp_thread.read(cx).entries().len()
    }

    fn send(&self, message: String, cx: &AsyncApp) -> Task<Result<String>> {
        let thread = self.subagent_thread.clone();
        let acp_thread = self.acp_thread.clone();
        let subagent_session_id = self.session_id.clone();
        let parent_thread = self.parent_thread.clone();

        cx.spawn(async move |cx| {
            let (task, _subscription) = cx.update(|cx| {
                let ratio_before_prompt = thread
                    .read(cx)
                    .latest_token_usage()
                    .map(|usage| usage.ratio());

                parent_thread
                    .update(cx, |parent_thread, _cx| {
                        parent_thread.register_running_subagent(thread.downgrade())
                    })
                    .ok();

                let task = acp_thread.update(cx, |acp_thread, cx| {
                    acp_thread.send(vec![message.into()], cx)
                });

                let (token_limit_tx, token_limit_rx) = oneshot::channel::<()>();
                let mut token_limit_tx = Some(token_limit_tx);

                let subscription = cx.subscribe(
                    &thread,
                    move |_thread, event: &TokenUsageUpdated, _cx| {
                        if let Some(usage) = &event.0 {
                            let old_ratio = ratio_before_prompt
                                .clone()
                                .unwrap_or(TokenUsageRatio::Normal);
                            let new_ratio = usage.ratio();
                            if old_ratio == TokenUsageRatio::Normal
                                && new_ratio == TokenUsageRatio::Warning
                            {
                                if let Some(tx) = token_limit_tx.take() {
                                    tx.send(()).ok();
                                }
                            }
                        }
                    },
                );

                let wait_for_prompt = cx
                    .background_spawn(async move {
                        futures::select! {
                            response = task.fuse() => match response {
                                Ok(Some(response)) => {
                                    match response.stop_reason {
                                        acp::StopReason::Cancelled => SubagentPromptResult::Cancelled,
                                        acp::StopReason::MaxTokens => SubagentPromptResult::Error("The agent reached the maximum number of tokens.".into()),
                                        acp::StopReason::MaxTurnRequests => SubagentPromptResult::Error("The agent reached the maximum number of allowed requests between user turns. Try prompting again.".into()),
                                        acp::StopReason::Refusal => SubagentPromptResult::Error("The agent refused to process that prompt. Try again.".into()),
                                        acp::StopReason::EndTurn | _ => SubagentPromptResult::Completed,
                                    }
                                }
                                Ok(None) => SubagentPromptResult::Error("No response from the agent. You can try messaging again.".into()),
                                Err(error) => SubagentPromptResult::Error(error.to_string()),
                            },
                            _ = token_limit_rx.fuse() => SubagentPromptResult::ContextWindowWarning,
                        }
                    });

                (wait_for_prompt, subscription)
            });

            let result = match task.await {
                SubagentPromptResult::Completed => thread.read_with(cx, |thread, _cx| {
                    thread
                        .last_message()
                        .and_then(|message| {
                            let content = message.as_agent_message()?
                                .content
                                .iter()
                                .filter_map(|c| match c {
                                    AgentMessageContent::Text(text) => Some(text.as_str()),
                                    _ => None,
                                })
                                .join("\n\n");
                            if content.is_empty() {
                                None
                            } else {
                                Some( content)
                            }
                        })
                        .context("No response from subagent")
                }),
                SubagentPromptResult::Cancelled => Err(anyhow!("User canceled")),
                SubagentPromptResult::Error(message) => Err(anyhow!("{message}")),
                SubagentPromptResult::ContextWindowWarning => {
                    thread.update(cx, |thread, cx| thread.cancel(cx)).await;
                    Err(anyhow!(
                        "The agent is nearing the end of its context window and has been \
                         stopped. You can prompt the thread again to have the agent wrap up \
                         or hand off its work."
                    ))
                }
            };

            parent_thread
                .update(cx, |parent_thread, cx| {
                    parent_thread.unregister_running_subagent(&subagent_session_id, cx)
                })
                .ok();

            result
        })
    }
}

pub struct AcpTerminalHandle {
    terminal: Entity<acp_thread::Terminal>,
    _drop_tx: Option<oneshot::Sender<()>>,
}

impl TerminalHandle for AcpTerminalHandle {
    fn id(&self, cx: &AsyncApp) -> Result<acp::TerminalId> {
        Ok(self.terminal.read_with(cx, |term, _cx| term.id().clone()))
    }

    fn wait_for_exit(&self, cx: &AsyncApp) -> Result<Shared<Task<acp::TerminalExitStatus>>> {
        Ok(self
            .terminal
            .read_with(cx, |term, _cx| term.wait_for_exit()))
    }

    fn current_output(&self, cx: &AsyncApp) -> Result<acp::TerminalOutputResponse> {
        Ok(self
            .terminal
            .read_with(cx, |term, cx| term.current_output(cx)))
    }

    fn kill(&self, cx: &AsyncApp) -> Result<()> {
        cx.update(|cx| {
            self.terminal.update(cx, |terminal, cx| {
                terminal.kill(cx);
            });
        });
        Ok(())
    }

    fn was_stopped_by_user(&self, cx: &AsyncApp) -> Result<bool> {
        Ok(self
            .terminal
            .read_with(cx, |term, _cx| term.was_stopped_by_user()))
    }
}
