use super::*;

impl MessageEditor {
    pub fn insert_dragged_files(
        &mut self,
        paths: Vec<project::ProjectPath>,
        added_worktrees: Vec<Entity<Worktree>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace) = self.workspace.upgrade() else {
            return;
        };
        let project = workspace.read(cx).project().clone();
        let supports_images = self.session_capabilities.read().supports_images();
        let mut tasks = Vec::new();
        for path in paths {
            if let Some(task) = insert_mention_for_project_path(
                &path,
                &self.editor,
                &self.mention_set,
                &project,
                &workspace,
                supports_images,
                window,
                cx,
            ) {
                tasks.push(task);
            }
        }
        cx.spawn(async move |_, _| {
            join_all(tasks).await;
            drop(added_worktrees);
        })
        .detach();
    }

    pub fn insert_branch_diff_crease(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(workspace) = self.workspace.upgrade() else {
            return;
        };

        let project = workspace.read(cx).project().clone();

        let Some(repo) = project.read(cx).active_repository(cx) else {
            return;
        };

        let default_branch_receiver = repo.update(cx, |repo, _| repo.default_branch(false));
        let editor = self.editor.clone();
        let mention_set = self.mention_set.clone();
        let weak_workspace = self.workspace.clone();

        window
            .spawn(cx, async move |cx| {
                let base_ref: SharedString = default_branch_receiver
                    .await
                    .ok()
                    .and_then(|r| r.ok())
                    .flatten()
                    .ok_or_else(|| anyhow!("Could not determine default branch"))?;

                cx.update(|window, cx| {
                    let mention_uri = MentionUri::GitDiff {
                        base_ref: base_ref.to_string(),
                    };
                    let mention_text = mention_uri.as_link().to_string();

                    let (text_anchor, content_len) = editor.update(cx, |editor, cx| {
                        let buffer = editor.buffer().read(cx);
                        let snapshot = buffer.snapshot(cx);
                        let buffer_snapshot = snapshot.as_singleton().unwrap();
                        let text_anchor = snapshot
                            .anchor_to_buffer_anchor(editor.selections.newest_anchor().start)
                            .unwrap()
                            .0
                            .bias_left(&buffer_snapshot);

                        editor.insert(&mention_text, window, cx);
                        editor.insert(" ", window, cx);

                        (text_anchor, mention_text.len())
                    });

                    let Some((crease_id, tx, crease_entity)) = insert_crease_for_mention(
                        text_anchor,
                        content_len,
                        mention_uri.name().into(),
                        mention_uri.icon_path(cx),
                        mention_uri.tooltip_text(),
                        Some(mention_uri.clone()),
                        Some(weak_workspace),
                        None,
                        editor,
                        window,
                        cx,
                    ) else {
                        return;
                    };
                    drop(tx);

                    let confirm_task = mention_set.update(cx, |mention_set, cx| {
                        mention_set.confirm_mention_for_git_diff(base_ref, cx)
                    });

                    let mention_task = cx
                        .spawn(async move |_cx| confirm_task.await.map_err(|e| e.to_string()))
                        .shared();

                    mention_set.update(cx, |mention_set, cx| {
                        mention_set.insert_mention(
                            crease_id,
                            mention_uri,
                            mention_task,
                            crease_entity,
                            cx,
                        );
                    });
                })
            })
            .detach_and_log_err(cx);
    }

    pub fn insert_skill_crease(
        &mut self,
        skill: &AvailableSkill,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace) = self.workspace.upgrade() else {
            return;
        };

        let mention_uri = MentionUri::Skill {
            name: skill.name.to_string(),
            source: skill.source.to_string(),
            skill_file_path: skill.skill_file_path.clone(),
        };

        let link_text = mention_uri.as_link().to_string();
        let content_len = link_text.len();
        let mention_text = format!("{} ", link_text);
        let crease_text: SharedString = mention_uri.name().into();

        let start_anchor = self.editor.update(cx, |editor, cx| {
            let snapshot = editor.buffer().read(cx).snapshot(cx);
            let buffer_snapshot = snapshot.as_singleton()?;
            let cursor = editor.selections.newest_anchor().start;
            let text_anchor = snapshot
                .anchor_to_buffer_anchor(cursor)?
                .0
                .bias_left(buffer_snapshot);

            editor.insert(&mention_text, window, cx);
            Some(text_anchor)
        });

        let Some(start_anchor) = start_anchor else {
            return;
        };

        self.mention_set
            .update(cx, |mention_set, cx| {
                mention_set.confirm_mention_completion(
                    crease_text,
                    start_anchor,
                    content_len,
                    mention_uri,
                    false,
                    self.editor.clone(),
                    &workspace,
                    window,
                    cx,
                )
            })
            .detach();
    }

    pub(crate) fn insert_selections(
        &mut self,
        selection: AgentContextSelection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let editor = self.editor.read(cx);
        let editor_buffer = editor.buffer().read(cx);
        let Some(buffer) = editor_buffer.as_singleton() else {
            return;
        };
        let cursor_anchor = editor.selections.newest_anchor().head();
        let cursor_offset = cursor_anchor.to_offset(&editor_buffer.snapshot(cx));
        let anchor = buffer.update(cx, |buffer, _cx| {
            buffer.anchor_before(cursor_offset.0.min(buffer.len()))
        });
        let Some(completion) =
            PromptCompletionProvider::<MessageEditorCompletionDelegate>::completion_for_action(
                PromptContextAction::AddSelections,
                anchor..anchor,
                self.editor.downgrade(),
                self.mention_set.downgrade(),
                Some(selection),
            )
        else {
            return;
        };

        self.editor.update(cx, |message_editor, cx| {
            message_editor.edit([(cursor_anchor..cursor_anchor, completion.new_text)], cx);
            message_editor.request_autoscroll(Autoscroll::fit(), cx);
        });
        if let Some(confirm) = completion.confirm {
            confirm(CompletionIntent::Complete, window, cx);
        }
    }

    pub fn add_images_from_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.session_capabilities.read().supports_images() {
            return;
        }

        let editor = self.editor.clone();
        let mention_set = self.mention_set.clone();
        let workspace = self.workspace.clone();

        let paths_receiver = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Select Images".into()),
        });

        window
            .spawn(cx, async move |cx| {
                let paths = match paths_receiver.await {
                    Ok(Ok(Some(paths))) => paths,
                    _ => return Ok::<(), anyhow::Error>(()),
                };

                let default_image_name: SharedString = "Image".into();
                let images = cx
                    .background_spawn(async move {
                        paths
                            .into_iter()
                            .filter_map(|path| {
                                crate::mention_set::load_external_image_from_path(
                                    &path,
                                    &default_image_name,
                                )
                            })
                            .collect::<Vec<_>>()
                    })
                    .await;

                crate::mention_set::insert_images_as_context(
                    images,
                    editor,
                    mention_set,
                    workspace,
                    cx,
                )
                .await;
                Ok(())
            })
            .detach_and_log_err(cx);
    }
}
