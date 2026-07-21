use super::*;

impl GitPanel {
    pub(super) fn show_error_toast(
        &self,
        action: impl Into<SharedString>,
        e: anyhow::Error,
        cx: &mut App,
    ) {
        let Some(workspace) = self.workspace.upgrade() else {
            return;
        };
        show_error_toast(workspace, action, e, cx)
    }

    pub(super) fn show_git_job_queue(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(repo) = self.active_repository.as_ref() else {
            let workspace = self.workspace.clone();
            cx.defer(move |cx| {
                if let Some(workspace) = workspace.upgrade() {
                    workspace.update(cx, |workspace, cx| {
                        struct GitJobQueueToast;
                        workspace.show_toast(
                            workspace::Toast::new(
                                NotificationId::unique::<GitJobQueueToast>(),
                                "No active repository",
                            )
                            .autohide(),
                            cx,
                        );
                    });
                }
            });
            return;
        };

        let repo_path = repo.read(cx).work_directory_abs_path.display().to_string();
        let queue_value = repo.read(cx).job_debug_queue().to_debug_value();
        let title = format!("Git Job Queue: {repo_path}");

        let json_language = self.project.read(cx).languages().language_for_name("JSON");
        let project = self.project.clone();
        let workspace = self.workspace.clone();

        window
            .spawn(cx, async move |cx| {
                let json_language = json_language.await.ok();

                // Best-effort: gather runtime diagnostics off the main thread.
                // Any failure inside `gather` is logged and produces an empty
                // section; this `.await` itself cannot meaningfully fail and
                // must never prevent us from showing the queue dump.
                let diagnostics = cx
                    .background_spawn(crate::git_runtime_diagnostics::gather())
                    .await;

                let mut combined = queue_value;
                if let serde_json::Value::Object(ref mut map) = combined
                    && let serde_json::Value::Object(diag_map) = diagnostics
                    && !diag_map.is_empty()
                {
                    map.insert(
                        "runtime_diagnostics".into(),
                        serde_json::Value::Object(diag_map),
                    );
                }

                let text = serde_json::to_string_pretty(&combined).unwrap_or_default();

                let buffer = project
                    .update(cx, |project, cx| {
                        project.create_buffer(json_language, false, cx)
                    })
                    .await?;

                buffer.update(cx, |buffer, cx| {
                    buffer.set_text(text, cx);
                    buffer.set_capability(language::Capability::ReadWrite, cx);
                });

                workspace.update_in(cx, |workspace, window, cx| {
                    let buffer =
                        cx.new(|cx| MultiBuffer::singleton(buffer, cx).with_title(title.clone()));

                    workspace.add_item_to_active_pane(
                        Box::new(cx.new(|cx| {
                            let mut editor =
                                Editor::for_multibuffer(buffer, Some(project.clone()), window, cx);
                            editor.set_breadcrumb_header(title);
                            editor.disable_mouse_wheel_zoom();
                            editor
                        })),
                        None,
                        true,
                        window,
                        cx,
                    );
                })?;

                anyhow::Ok(())
            })
            .detach_and_log_err(cx);
    }

    pub(super) fn show_commit_message_error<E>(
        weak_this: &WeakEntity<Self>,
        err: &E,
        cx: &mut AsyncApp,
    ) where
        E: std::fmt::Debug + std::fmt::Display,
    {
        if let Ok(Some(workspace)) = weak_this.update(cx, |this, _cx| this.workspace.upgrade()) {
            let _ = workspace.update(cx, |workspace, cx| {
                workspace.show_error(format!("Failed to generate commit message: {err}"), cx);
            });
        }
    }

    pub(super) fn show_remote_output(
        &mut self,
        action: RemoteAction,
        info: RemoteCommandOutput,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace) = self.workspace.upgrade() else {
            return;
        };

        let is_push = matches!(action, RemoteAction::Push(_, _));

        workspace.update(cx, |workspace, cx| {
            let SuccessMessage { message, style } = remote_output::format_output(&action, info);
            let workspace_weak = cx.weak_entity();
            let operation = action.name();

            let status_toast = StatusToast::new(message, cx, move |this, _cx| {
                use remote_output::SuccessStyle::*;
                let this = this.icon(
                    Icon::new(IconName::GitBranch)
                        .size(IconSize::Small)
                        .color(Color::Muted),
                );
                match (style, is_push) {
                    (Toast | ToastWithLog { .. }, true) => {
                        this.action("Create Pull Request", move |window, cx| {
                            window
                                .dispatch_action(Box::new(mav_actions::git::CreatePullRequest), cx);
                        })
                    }
                    (Toast, false) => this,
                    (ToastWithLog { output }, false) => {
                        this.action("View Log", move |window, cx| {
                            let output = output.clone();
                            let output =
                                format!("stdout:\n{}\nstderr:\n{}", output.stdout, output.stderr);
                            workspace_weak
                                .update(cx, move |workspace, cx| {
                                    open_output(operation, workspace, &output, window, cx)
                                })
                                .ok();
                        })
                    }
                }
                .dismiss_button(true)
            });
            workspace.toggle_status_toast(status_toast, cx)
        });
    }

    pub fn can_commit(&self) -> bool {
        (self.has_staged_changes() || self.has_tracked_changes()) && !self.has_unstaged_conflicts()
    }

    pub fn can_stage_all(&self) -> bool {
        self.has_unstaged_changes()
    }

    pub fn can_unstage_all(&self) -> bool {
        self.has_staged_changes()
    }
}
