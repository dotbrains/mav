use super::*;

impl GitPanel {
    pub(super) fn serialization_key(workspace: &Workspace) -> Option<String> {
        workspace
            .database_id()
            .map(|id| i64::from(id).to_string())
            .or(workspace.session_id())
            .map(|id| format!("{}-{:?}", GIT_PANEL_KEY, id))
    }

    pub(super) fn serialize(&mut self, cx: &mut Context<Self>) {
        let signoff_enabled = self.signoff_enabled;
        let commit_messages = self.serialized_commit_messages(cx);
        let kvp = KeyValueStore::global(cx);

        self.pending_serialization = cx.spawn(async move |git_panel, cx| {
            cx.background_executor()
                .timer(SERIALIZATION_THROTTLE_TIME)
                .await;
            let Some(serialization_key) = git_panel
                .update(cx, |git_panel, cx| {
                    git_panel
                        .workspace
                        .read_with(cx, |workspace, _| Self::serialization_key(workspace))
                        .ok()
                        .flatten()
                })
                .ok()
                .flatten()
            else {
                return;
            };
            cx.background_spawn(
                async move {
                    kvp.write_kvp(
                        serialization_key,
                        serde_json::to_string(&SerializedGitPanel {
                            signoff_enabled,
                            commit_messages,
                        })?,
                    )
                    .await?;
                    anyhow::Ok(())
                }
                .log_err(),
            )
            .await;
        });
    }

    fn serialized_commit_messages(&self, cx: &App) -> BTreeMap<String, SerializedCommitMessage> {
        let active_work_directory_abs_path = self.active_repository.as_ref().map(|repository| {
            repository
                .read(cx)
                .work_directory_abs_path
                .to_string_lossy()
                .into_owned()
        });
        let git_store = self.project.read(cx).git_store().clone();
        let mut commit_messages = self.pending_commit_message_restores.clone();
        for repository in git_store.read(cx).repositories().values() {
            let repository = repository.read(cx);
            let work_directory_abs_path = repository
                .work_directory_abs_path
                .to_string_lossy()
                .into_owned();
            if active_work_directory_abs_path.as_deref() == Some(work_directory_abs_path.as_str()) {
                continue;
            }
            if let Some(buffer) = repository.commit_message_buffer() {
                let text = buffer.read(cx).text();
                if text.trim().is_empty() {
                    commit_messages.remove(&work_directory_abs_path);
                } else {
                    commit_messages.insert(
                        work_directory_abs_path,
                        SerializedCommitMessage {
                            message: Some(text),
                            original_message: None,
                            amend_pending: false,
                        },
                    );
                }
            }
        }
        if let Some(work_directory_abs_path) = active_work_directory_abs_path {
            let text = self.commit_message_buffer(cx).read(cx).text();
            let message = (!text.trim().is_empty()).then_some(text);
            let original_message = self.original_commit_message.clone();
            let amend_pending = self.amend_pending;
            if message.is_some() || original_message.is_some() || amend_pending {
                commit_messages.insert(
                    work_directory_abs_path,
                    SerializedCommitMessage {
                        message,
                        original_message,
                        amend_pending,
                    },
                );
            } else {
                commit_messages.remove(&work_directory_abs_path);
            }
        }
        commit_messages
    }

    pub(crate) fn set_modal_open(&mut self, open: bool, cx: &mut Context<Self>) {
        self.modal_open = open;
        cx.notify();
    }

    pub(super) fn dispatch_context(&self, window: &mut Window, cx: &Context<Self>) -> KeyContext {
        let mut dispatch_context = KeyContext::new_with_defaults();
        dispatch_context.add("GitPanel");

        if self.commit_editor.read(cx).is_focused(window) {
            dispatch_context.add("CommitEditor");
        } else if self.focus_handle.contains_focused(window, cx) {
            dispatch_context.add("menu");
            dispatch_context.add("ChangesList");
        }

        dispatch_context
    }
}
