use super::*;

impl AcpThread {
    pub fn create_terminal(
        &self,
        command: String,
        args: Vec<String>,
        extra_env: Vec<acp::EnvVariable>,
        cwd: Option<PathBuf>,
        output_byte_limit: Option<u64>,
        sandbox_wrap: Option<SandboxWrap>,
        cx: &mut Context<Self>,
    ) -> Task<Result<Entity<Terminal>>> {
        let env = match &cwd {
            Some(dir) => self.project.update(cx, |project, cx| {
                project.environment().update(cx, |env, cx| {
                    env.directory_environment(dir.as_path().into(), cx)
                })
            }),
            None => Task::ready(None).shared(),
        };
        let env = cx.spawn(async move |_, _| {
            let mut env = env.await.unwrap_or_default();
            // Disables paging for `git` and hopefully other commands
            env.insert("PAGER".into(), "".into());
            for var in extra_env {
                env.insert(var.name, var.value);
            }
            env
        });

        let project = self.project.clone();
        let language_registry = project.read(cx).languages().clone();
        let is_windows = project.read(cx).path_style(cx).is_windows();
        // Headless hosts (e.g. the eval CLI) have no controlling TTY, so PTY
        // setup fails with `ENOTTY`. Run the command non-interactively and
        // without a PTY in that case.
        let headless = HeadlessTerminal::is_enabled(cx);

        let terminal_id = acp::TerminalId::new(Uuid::new_v4().to_string());
        let terminal_task = cx.spawn({
            let terminal_id = terminal_id.clone();
            async move |_this, cx| {
                let env = env.await;
                let shell = project
                    .update(cx, |project, cx| {
                        project
                            .remote_client()
                            .and_then(|r| r.read(cx).default_system_shell())
                    })
                    .unwrap_or_else(|| get_default_system_shell_preferring_bash());

                // The sandbox owns the network proxy (for restricted-network
                // policies) and injects the child's proxy env vars, returning
                // the env to spawn with. On Windows, restricted host access is
                // rejected inside the sandbox before command preparation.
                #[cfg(target_os = "windows")]
                let (task_command, task_args, task_env, sandbox, spawn_cwd) =
                    if sandbox_wrap.is_some() {
                        let (task_command, task_args) = task::ShellBuilder::new(
                            &Shell::Program("/bin/sh".to_string()),
                            false,
                        )
                        .non_interactive()
                        .redirect_stdin_to_dev_null()
                        .build(Some(command.clone()), &args);
                        let wrap = cx.background_spawn(prepare_sandbox_wrap(
                            task_command,
                            task_args,
                            cwd.clone(),
                            sandbox_wrap,
                            env,
                        ));
                        let timeout = cx.background_executor().timer(WSL_SANDBOX_WRAP_TIMEOUT);
                        let (task_command, task_args, task_env, sandbox) = futures::select_biased! {
                            result = wrap.fuse() => result?,
                            _ = timeout.fuse() => return Err(anyhow::Error::new(
                                sandbox::SandboxError::WslUnavailable(format!(
                                    "WSL did not respond within {} seconds while preparing the sandboxed command",
                                    WSL_SANDBOX_WRAP_TIMEOUT.as_secs()
                                )),
                            )),
                        };
                        (task_command, task_args, task_env, sandbox, None)
                    } else {
                        // No sandbox wrap means we're running unsandboxed, and
                        // on Windows that deliberately changes the shell: the
                        // sandboxed path runs under WSL's Linux bash, but this
                        // fallback uses the host's `shell` against the native cwd.
                        let mut builder = ShellBuilder::new(&Shell::Program(shell), is_windows);
                        if headless {
                            builder = builder.non_interactive();
                        }
                        let (task_command, task_args) = builder
                            .redirect_stdin_to_dev_null()
                            .build(Some(command.clone()), &args);
                        (task_command, task_args, env, None, cwd.clone())
                    };

                #[cfg(not(target_os = "windows"))]
                let (task_command, task_args, task_env, sandbox, spawn_cwd) = {
                    let mut builder = ShellBuilder::new(&Shell::Program(shell), is_windows);
                    if headless {
                        builder = builder.non_interactive();
                    }
                    let (task_command, task_args) = builder
                        .redirect_stdin_to_dev_null()
                        .build(Some(command.clone()), &args);
                    let (task_command, task_args, task_env, sandbox) = prepare_sandbox_wrap(
                        task_command,
                        task_args,
                        cwd.clone(),
                        sandbox_wrap,
                        env,
                    )
                    .await?;
                    (task_command, task_args, task_env, sandbox, cwd.clone())
                };
                let terminal = project
                    .update(cx, |project, cx| {
                        project.create_terminal_task(
                            task::SpawnInTerminal {
                                command: Some(task_command),
                                args: task_args,
                                cwd: spawn_cwd,
                                env: task_env,
                                ..Default::default()
                            },
                            cx,
                        )
                    })
                    .await?;

                anyhow::Ok(cx.new(|cx| {
                    Terminal::new(
                        terminal_id,
                        &format!("{} {}", command, args.join(" ")),
                        cwd,
                        output_byte_limit.map(|l| l as usize),
                        terminal,
                        language_registry,
                        sandbox,
                        cx,
                    )
                }))
            }
        });

        cx.spawn(async move |this, cx| {
            let terminal = terminal_task.await?;
            this.update(cx, |this, _cx| {
                this.terminals.insert(terminal_id, terminal.clone());
                terminal
            })
        })
    }

    pub fn kill_terminal(
        &mut self,
        terminal_id: acp::TerminalId,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        self.terminals
            .get(&terminal_id)
            .context("Terminal not found")?
            .update(cx, |terminal, cx| {
                terminal.kill(cx);
            });

        Ok(())
    }

    pub fn release_terminal(
        &mut self,
        terminal_id: acp::TerminalId,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        self.terminals
            .remove(&terminal_id)
            .context("Terminal not found")?
            .update(cx, |terminal, cx| {
                terminal.kill(cx);
            });

        Ok(())
    }

    pub fn terminal(&self, terminal_id: acp::TerminalId) -> Result<Entity<Terminal>> {
        self.terminals
            .get(&terminal_id)
            .context("Terminal not found")
            .cloned()
    }

    pub fn to_markdown(&self, cx: &App) -> String {
        self.entries.iter().map(|e| e.to_markdown(cx)).collect()
    }

    pub fn emit_load_error(&mut self, error: LoadError, cx: &mut Context<Self>) {
        cx.emit(AcpThreadEvent::LoadError(error));
    }

    pub fn register_terminal_created(
        &mut self,
        terminal_id: acp::TerminalId,
        command_label: String,
        working_dir: Option<PathBuf>,
        output_byte_limit: Option<u64>,
        terminal: Entity<::terminal::Terminal>,
        cx: &mut Context<Self>,
    ) -> Entity<Terminal> {
        let language_registry = self.project.read(cx).languages().clone();

        let entity = cx.new(|cx| {
            Terminal::new(
                terminal_id.clone(),
                &command_label,
                working_dir.clone(),
                output_byte_limit.map(|l| l as usize),
                terminal,
                language_registry,
                // External terminal providers manage their own sandboxing
                // (if any). We don't wrap their commands.
                None,
                cx,
            )
        });
        self.terminals.insert(terminal_id.clone(), entity.clone());
        entity
    }

    pub fn mark_as_subagent_output(&mut self, cx: &mut Context<Self>) {
        for entry in self.entries.iter_mut().rev() {
            if let AgentThreadEntry::AssistantMessage(assistant_message) = entry {
                assistant_message.is_subagent_output = true;
                cx.notify();
                return;
            }
        }
    }

    pub fn on_terminal_provider_event(
        &mut self,
        event: TerminalProviderEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            TerminalProviderEvent::Created {
                terminal_id,
                label,
                cwd,
                output_byte_limit,
                terminal,
            } => {
                let entity = self.register_terminal_created(
                    terminal_id.clone(),
                    label,
                    cwd,
                    output_byte_limit,
                    terminal,
                    cx,
                );

                if let Some(mut chunks) = self.pending_terminal_output.remove(&terminal_id) {
                    for data in chunks.drain(..) {
                        entity.update(cx, |term, cx| {
                            term.inner().update(cx, |inner, cx| {
                                inner.write_output(&data, cx);
                            })
                        });
                    }
                }

                if let Some(_status) = self.pending_terminal_exit.remove(&terminal_id) {
                    entity.update(cx, |_term, cx| {
                        cx.notify();
                    });
                }

                cx.notify();
            }
            TerminalProviderEvent::Output { terminal_id, data } => {
                if let Some(entity) = self.terminals.get(&terminal_id) {
                    entity.update(cx, |term, cx| {
                        term.inner().update(cx, |inner, cx| {
                            inner.write_output(&data, cx);
                        })
                    });
                } else {
                    self.pending_terminal_output
                        .entry(terminal_id)
                        .or_default()
                        .push(data);
                }
            }
            TerminalProviderEvent::TitleChanged { terminal_id, title } => {
                if let Some(entity) = self.terminals.get(&terminal_id) {
                    entity.update(cx, |term, cx| {
                        term.inner().update(cx, |inner, cx| {
                            inner.breadcrumb_text = title;
                            cx.emit(::terminal::Event::BreadcrumbsChanged);
                        })
                    });
                }
            }
            TerminalProviderEvent::Exit {
                terminal_id,
                status,
            } => {
                if let Some(entity) = self.terminals.get(&terminal_id) {
                    entity.update(cx, |_term, cx| {
                        cx.notify();
                    });
                } else {
                    self.pending_terminal_exit.insert(terminal_id, status);
                }
            }
        }
    }
}
