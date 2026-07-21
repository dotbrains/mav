use super::*;

pub(super) async fn run_terminal_tool(
    project: Entity<Project>,
    environment: Rc<dyn ThreadEnvironment>,
    input: TerminalToolRequest,
    event_stream: ToolCallEventStream,
    cx: &mut AsyncApp,
) -> Result<String, String> {
    let selection = input.selection;
    let sandbox_input = input.sandbox.clone().unwrap_or_default();

    let (working_dir, authorize, sandboxing, is_local_project) = cx.update(|cx| {
        let working_dir = working_dir(&input.cd, &project, cx).map_err(|err| err.to_string())?;
        let context =
            crate::ToolPermissionContext::new(TerminalTool::NAME, vec![input.command.clone()]);
        let authorize =
            event_stream.authorize(SharedString::new(input.command.clone()), context, cx);
        let sandboxing =
            input.sandbox.is_some() && sandboxing_enabled_for_project(project.read(cx), cx);
        let is_local_project = project.read(cx).is_local();
        Result::<_, String>::Ok((working_dir, authorize, sandboxing, is_local_project))
    })?;

    authorize.await.map_err(|e| e.to_string())?;

    let want_fs_write_all = sandboxing && sandbox_input.allow_fs_write_all == Some(true);
    let want_unsandboxed = sandboxing && sandbox_input.unsandboxed == Some(true);
    let want_all_hosts = sandboxing && sandbox_input.allow_all_hosts == Some(true);
    let want_git_access = sandboxing && sandbox_input.allow_git_access == Some(true);

    let persistent = cx.update(|cx| {
        agent_settings::AgentSettings::get_global(cx)
            .sandbox_permissions
            .clone()
    });

    let floor = event_stream
        .effective_sandbox_request(&crate::sandboxing::SandboxRequest::default(), &persistent);
    let unsandboxed_floor = sandboxing
        && (event_stream.unsandboxed_granted_for_thread()
            || event_stream.sandbox_fallback_granted_for_thread());
    let fs_unrestricted_floor = sandboxing && floor.allow_fs_write_all;
    let net_unrestricted_floor = sandboxing && matches!(floor.network, NetworkRequest::AnyHost);

    if sandboxing && !want_unsandboxed {
        if unsandboxed_floor {
            let mut ineffective = Vec::new();
            if !sandbox_input.allow_hosts.is_empty() {
                ineffective.push("`allow_hosts`");
            }
            if sandbox_input.allow_all_hosts == Some(true) {
                ineffective.push("`allow_all_hosts`");
            }
            if !sandbox_input.fs_write_paths.is_empty() {
                ineffective.push("`fs_write_paths`");
            }
            if sandbox_input.allow_fs_write_all == Some(true) {
                ineffective.push("`allow_fs_write_all`");
            }
            if !ineffective.is_empty() {
                return Err(format!(
                    "Sandboxing is disabled for this thread, so every command runs without an OS \
                     sandbox and these fields have no effect: {}. Remove them and rerun the \
                     command (it will run unsandboxed), or pass `unsandboxed: true` to acknowledge \
                     it runs without a sandbox.",
                    ineffective.join(", "),
                ));
            }
        } else {
            if fs_unrestricted_floor
                && !want_fs_write_all
                && !sandbox_input.fs_write_paths.is_empty()
            {
                return Err(
                    "Unrestricted filesystem writes are enabled for this thread, so every command \
                     can already write anywhere; `fs_write_paths` cannot narrow that. Remove \
                     `fs_write_paths`."
                        .to_string(),
                );
            }
            if net_unrestricted_floor && !want_all_hosts && !sandbox_input.allow_hosts.is_empty() {
                return Err(
                    "Unrestricted network access is enabled for this thread, so every command can \
                     already reach any host; `allow_hosts` cannot narrow that. Remove `allow_hosts`."
                        .to_string(),
                );
            }
        }
    }

    let network = if sandboxing && !want_unsandboxed {
        build_network_request(&sandbox_input)?
    } else {
        NetworkRequest::None
    };

    let can_restrict_to_hosts =
        (cfg!(target_os = "macos") || cfg!(target_os = "linux")) && is_local_project;
    if !can_restrict_to_hosts && matches!(network, NetworkRequest::Hosts(_)) {
        return Err(
            "This platform or project cannot restrict sandboxed network access to specific hosts. Use `allow_all_hosts: true` if the command needs network access."
                .to_string(),
        );
    }

    let write_paths: Vec<PathBuf> = if sandboxing && !want_unsandboxed {
        cx.update(|cx| {
            resolve_write_paths(
                &sandbox_input.fs_write_paths,
                working_dir.as_deref(),
                &project,
                cx,
            )
        })
    } else {
        Vec::new()
    };

    #[cfg(target_os = "linux")]
    for path in &write_paths {
        if !path.is_dir() {
            return Err(format!(
                "Cannot request sandbox write access to `{}`: on Linux, write access can only \
                 be granted to directories that already exist. To create or modify files, \
                 request write access to the existing directory that contains them, not the \
                 file path itself.",
                path.display()
            ));
        }
    }

    let request = crate::sandboxing::SandboxRequest {
        network,
        allow_git_access: !want_unsandboxed && want_git_access,
        allow_fs_write_all: !want_unsandboxed && want_fs_write_all,
        unsandboxed: want_unsandboxed,
        write_paths,
    };

    if request.needs_escalation() {
        let reason = sandbox_input
            .reason
            .as_deref()
            .map(str::trim)
            .filter(|reason| !reason.is_empty());
        let Some(reason) = reason else {
            return Err(
                "This command requests elevated sandbox permissions, so a `reason` is \
                 required: briefly justify why the command needs them, then run it again."
                    .to_string(),
            );
        };
        let approve =
            cx.update(|cx| event_stream.authorize_sandbox(request.clone(), reason.to_string(), cx));
        if let Err(error) = approve.await {
            if want_unsandboxed {
                return Ok(format!(
                    "Command cancelled: user denied permission to run outside the sandbox ({error})."
                ));
            }
            return Ok(format!(
                "Command cancelled: user denied the requested sandbox permissions ({error})."
            ));
        }
    }

    let extra_env = Vec::new();

    #[cfg_attr(
        not(any(target_os = "macos", target_os = "linux", target_os = "windows")),
        allow(unused_mut)
    )]
    let mut sandbox_not_applied: Option<acp_thread::SandboxNotAppliedReason> = None;
    let mut git_access_downgrade_note = None;
    let sandbox_wrap = if sandboxing && !want_unsandboxed {
        if unsandboxed_floor {
            sandbox_not_applied = Some(acp_thread::SandboxNotAppliedReason::DisabledForThisThread);
            None
        } else {
            let effective = event_stream.effective_sandbox_request(&request, &persistent);
            if !can_restrict_to_hosts && matches!(effective.network, NetworkRequest::Hosts(_)) {
                return Err(
                    "This platform or project has a saved host-specific network grant, but cannot enforce host-specific sandboxed network access. Request `allow_all_hosts: true` if the command needs network access."
                        .to_string(),
                );
            }
            let (fs, sandbox_path_candidates) = cx.update(|cx| {
                (
                    project.read(cx).fs().clone(),
                    SandboxGitPathCandidates::from_project(project.read(cx), cx),
                )
            });
            let sandbox_paths = sandbox_git_paths(
                sandbox_path_candidates,
                fs.as_ref(),
                effective.allow_git_access,
            )
            .await;
            if effective.allow_git_access && !sandbox_paths.allow_git_access {
                log::warn!(
                    "Downgrading requested agent terminal Git metadata access because one or more external Git metadata paths could not be verified"
                );
                git_access_downgrade_note = Some(
                    "Note: Git metadata access was requested or already allowed, but Mav could not verify one or more external Git metadata paths for this project. The command ran with Git metadata protected, so Git operations that read or write `.git` may fail with sandbox permission errors."
                        .to_string(),
                );
            }
            let wrap = acp_thread::SandboxWrap {
                writable_paths: sandbox_paths.writable_paths,
                extra_write_paths: effective.write_paths,
                git_dirs: sandbox_paths.git_dirs,
                allow_git_access: sandbox_paths.allow_git_access,
                network: network_request_to_sandbox_network_access(&effective.network),
                allow_fs_write: effective.allow_fs_write_all,
                is_local: is_local_project,
            };

            #[cfg(target_os = "linux")]
            {
                let mut retries = 0usize;
                loop {
                    let probe_wrap = wrap.clone();
                    let probe_cwd = working_dir.clone();
                    let error = match cx
                        .background_executor()
                        .spawn(async move { probe_wrap.can_create_sandbox(probe_cwd.as_deref()) })
                        .await
                    {
                        Ok(()) => break Some(wrap),
                        Err(error) => error,
                    };

                    log::warn!(
                        "Failed to create a sandbox for an agent terminal command: {error:?}"
                    );

                    let decision = cx
                        .update(|cx| {
                            event_stream.authorize_sandbox_fallback(
                                Some(input.command.clone()),
                                error.user_facing_message(),
                                retries,
                                cx,
                            )
                        })
                        .await;
                    match decision {
                        Ok(SandboxFallbackDecision::Retry) => {
                            retries += 1;
                            continue;
                        }
                        Ok(SandboxFallbackDecision::RunUnsandboxed) => {
                            sandbox_not_applied =
                                Some(acp_thread::SandboxNotAppliedReason::ErrorLinuxWsl(error));
                            break None;
                        }
                        Ok(SandboxFallbackDecision::Deny) | Err(_) => {
                            return Ok(format!(
                                "Command cancelled: the sandbox could not be created ({}) and \
                                 the user declined to run it without one.",
                                error.user_facing_message()
                            ));
                        }
                    }
                }
            }
            #[cfg(not(target_os = "linux"))]
            {
                let probe_wrap = wrap.clone();
                let probe_cwd = working_dir.clone();
                match cx
                    .background_executor()
                    .spawn(async move { probe_wrap.can_create_sandbox(probe_cwd.as_deref()) })
                    .await
                {
                    Ok(()) => Some(wrap),
                    Err(error) => {
                        log::warn!(
                            "Failed to create a sandbox for an agent terminal command: {error:?}"
                        );
                        None
                    }
                }
            }
        }
    } else {
        None
    };

    let output_byte_limit = if selection.is_enabled() {
        None
    } else {
        Some(COMMAND_OUTPUT_LIMIT)
    };

    #[cfg(target_os = "windows")]
    let terminal = {
        let mut retries = 0usize;
        let mut effective_wrap = sandbox_wrap.clone();
        loop {
            let error = match environment
                .create_terminal(
                    input.command.clone(),
                    extra_env.clone(),
                    working_dir.clone(),
                    output_byte_limit,
                    effective_wrap.clone(),
                    cx,
                )
                .await
            {
                Ok(terminal) => break terminal,
                Err(error) => error,
            };

            let Some(message) = effective_wrap.as_ref().and_then(|_| {
                error
                    .downcast_ref::<sandbox::SandboxError>()
                    .and_then(|error| match error {
                        sandbox::SandboxError::WslUnavailable(message) => Some(message.clone()),
                        _ => None,
                    })
            }) else {
                return Err(format!("{error:#}"));
            };
            let sandbox_error = acp_thread::LinuxWslSandboxError::Other(message);
            log::warn!("Failed to create a WSL sandbox for an agent terminal command: {error:?}");

            let decision = cx
                .update(|cx| {
                    event_stream.authorize_sandbox_fallback(
                        Some(input.command.clone()),
                        sandbox_error.user_facing_message(),
                        retries,
                        cx,
                    )
                })
                .await;
            match decision {
                Ok(SandboxFallbackDecision::Retry) => {
                    retries += 1;
                }
                Ok(SandboxFallbackDecision::RunUnsandboxed) => {
                    sandbox_not_applied = Some(acp_thread::SandboxNotAppliedReason::ErrorLinuxWsl(
                        sandbox_error,
                    ));
                    effective_wrap = None;
                }
                Ok(SandboxFallbackDecision::Deny) | Err(_) => {
                    return Ok(format!(
                        "Command cancelled: the sandbox could not be created ({}) and the \
                         user declined to run it without one.",
                        sandbox_error.user_facing_message()
                    ));
                }
            }
        }
    };
    #[cfg(not(target_os = "windows"))]
    let terminal = environment
        .create_terminal(
            input.command.clone(),
            extra_env,
            working_dir.clone(),
            output_byte_limit,
            sandbox_wrap.clone(),
            cx,
        )
        .await
        .map_err(|e| format!("{e:#}"))?;

    let sandbox_note = sandbox_not_applied.as_ref().map(|reason| {
        #[cfg_attr(not(target_os = "windows"), allow(unused_mut))]
        let mut note = match reason {
            acp_thread::SandboxNotAppliedReason::DisabledForThisThread => {
                "Note: this command ran WITHOUT an OS sandbox because the user allowed unsandboxed \
                 execution for the rest of this thread."
                    .to_string()
            }
            acp_thread::SandboxNotAppliedReason::ErrorLinuxWsl(error) => format!(
                "Note: this command ran WITHOUT an OS sandbox because one could not be \
                 created ({}).",
                error.user_facing_message()
            ),
        };
        #[cfg(target_os = "windows")]
        {
            note.push(' ');
            note.push_str(
                "It also ran under the host shell (Git Bash, or PowerShell/cmd when no bash is \
                 installed) instead of WSL's Linux shell, so the interpreter and path \
                 conventions differ from the sandbox: Linux-only commands and `/mnt/...` paths \
                 may fail. Rewrite the command for the host shell if it doesn't work.",
            );
        }
        note
    });

    let terminal_id = terminal.id(cx).map_err(|e| e.to_string())?;
    let fields = acp::ToolCallUpdateFields::new().content(vec![acp::ToolCallContent::Terminal(
        acp::Terminal::new(terminal_id),
    )]);
    if let Some(reason) = &sandbox_not_applied {
        event_stream.update_fields_with_meta(
            fields,
            Some(acp_thread::meta_with_sandbox_not_applied(reason)),
        );
    } else {
        event_stream.update_fields(fields);
    }

    let timeout = input.timeout_ms.map(Duration::from_millis);

    let mut timed_out = false;
    let mut user_stopped_via_signal = false;
    let wait_for_exit = terminal.wait_for_exit(cx).map_err(|e| e.to_string())?;

    match timeout {
        Some(timeout) => {
            let timeout_task = cx.background_executor().timer(timeout);

            futures::select! {
                _ = wait_for_exit.clone().fuse() => {},
                _ = timeout_task.fuse() => {
                    timed_out = true;
                    terminal.kill(cx).map_err(|e| e.to_string())?;
                    wait_for_exit.await;
                }
                _ = event_stream.cancelled_by_user().fuse() => {
                    user_stopped_via_signal = true;
                    terminal.kill(cx).map_err(|e| e.to_string())?;
                    wait_for_exit.await;
                }
            }
        }
        None => {
            futures::select! {
                _ = wait_for_exit.clone().fuse() => {},
                _ = event_stream.cancelled_by_user().fuse() => {
                    user_stopped_via_signal = true;
                    terminal.kill(cx).map_err(|e| e.to_string())?;
                    wait_for_exit.await;
                }
            }
        }
    };

    let user_stopped_via_signal = user_stopped_via_signal || event_stream.was_cancelled_by_user();
    let user_stopped_via_terminal = terminal.was_stopped_by_user(cx).unwrap_or(false);
    let user_stopped = user_stopped_via_signal || user_stopped_via_terminal;

    let output = terminal.current_output(cx).map_err(|e| e.to_string())?;

    let result = process_content(output, &input.command, timed_out, user_stopped, selection);
    let git_access_downgrade_note = (sandbox_wrap.is_some() && sandbox_not_applied.is_none())
        .then_some(git_access_downgrade_note)
        .flatten();
    let notes = sandbox_note
        .into_iter()
        .chain(git_access_downgrade_note)
        .collect::<Vec<_>>();
    Ok(if notes.is_empty() {
        result
    } else {
        format!("{}\n\n{result}", notes.join("\n\n"))
    })
}
