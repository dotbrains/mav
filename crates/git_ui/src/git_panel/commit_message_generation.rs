use super::*;

impl GitPanel {
    /// Suggests a commit message based on the changed files and their statuses
    pub fn suggest_commit_message(&self, cx: &App) -> Option<String> {
        if let Some(merge_message) = self
            .active_repository
            .as_ref()
            .and_then(|repo| repo.read(cx).merge.message.as_ref())
        {
            return Some(merge_message.to_string());
        }

        let git_status_entry = if let Some(staged_entry) = &self.single_staged_entry {
            Some(staged_entry)
        } else if self.total_staged_count() == 0
            && let Some(single_tracked_entry) = &self.single_tracked_entry
        {
            Some(single_tracked_entry)
        } else {
            None
        }?;

        let action_text = if git_status_entry.status.is_deleted() {
            Some("Delete")
        } else if git_status_entry.status.is_created() {
            Some("Create")
        } else if git_status_entry.status.is_modified() {
            Some("Update")
        } else {
            None
        }?;

        let file_name = git_status_entry
            .repo_path
            .file_name()
            .unwrap_or_default()
            .to_string();

        Some(format!("{} {}", action_text, file_name))
    }

    pub(super) fn generate_commit_message_action(
        &mut self,
        _: &git::GenerateCommitMessage,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.generate_commit_message(cx);
    }

    fn split_patch(patch: &str) -> Vec<String> {
        let mut result = Vec::new();
        let mut current_patch = String::new();

        for line in patch.lines() {
            if line.starts_with("---") && !current_patch.is_empty() {
                result.push(current_patch.trim_end_matches('\n').into());
                current_patch = String::new();
            }
            current_patch.push_str(line);
            current_patch.push('\n');
        }

        if !current_patch.is_empty() {
            result.push(current_patch.trim_end_matches('\n').into());
        }

        result
    }
    fn truncate_iteratively(patch: &str, max_bytes: usize) -> String {
        let mut current_size = patch.len();
        if current_size <= max_bytes {
            return patch.to_string();
        }
        let file_patches = Self::split_patch(patch);
        let mut file_infos: Vec<TruncatedPatch> = file_patches
            .iter()
            .filter_map(|patch| TruncatedPatch::from_unified_diff(patch))
            .collect();

        if file_infos.is_empty() {
            return patch.to_string();
        }

        current_size = file_infos.iter().map(|f| f.calculate_size()).sum::<usize>();
        while current_size > max_bytes {
            let file_idx = file_infos
                .iter()
                .enumerate()
                .filter(|(_, f)| f.hunks_to_keep > 1)
                .max_by_key(|(_, f)| f.hunks_to_keep)
                .map(|(idx, _)| idx);
            match file_idx {
                Some(idx) => {
                    let file = &mut file_infos[idx];
                    let size_before = file.calculate_size();
                    file.hunks_to_keep -= 1;
                    let size_after = file.calculate_size();
                    let saved = size_before.saturating_sub(size_after);
                    current_size = current_size.saturating_sub(saved);
                }
                None => {
                    break;
                }
            }
        }

        file_infos
            .iter()
            .map(|info| info.to_string())
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn compress_commit_diff(diff_text: &str, max_bytes: usize) -> String {
        if diff_text.len() <= max_bytes {
            return diff_text.to_string();
        }

        let mut compressed = diff_text
            .lines()
            .map(|line| {
                if line.len() > 256 {
                    format!("{}...[truncated]\n", &line[..line.floor_char_boundary(256)])
                } else {
                    format!("{}\n", line)
                }
            })
            .collect::<Vec<_>>()
            .concat();

        if compressed.len() <= max_bytes {
            return compressed;
        }

        compressed = Self::truncate_iteratively(&compressed, max_bytes);

        compressed
    }

    async fn load_project_rules(
        project: &Entity<Project>,
        repo_work_dir: &Arc<Path>,
        cx: &mut AsyncApp,
    ) -> Option<String> {
        let rules_path = cx.update(|cx| {
            for worktree in project.read(cx).worktrees(cx) {
                let worktree_abs_path = worktree.read(cx).abs_path();
                if !worktree_abs_path.starts_with(&repo_work_dir) {
                    continue;
                }

                let worktree_snapshot = worktree.read(cx).snapshot();
                for rules_name in RULES_FILE_NAMES {
                    if let Ok(rel_path) = RelPath::unix(rules_name) {
                        if let Some(entry) = worktree_snapshot.entry_for_path(rel_path) {
                            if entry.is_file() {
                                return Some(ProjectPath {
                                    worktree_id: worktree.read(cx).id(),
                                    path: entry.path.clone(),
                                });
                            }
                        }
                    }
                }
            }
            None
        })?;

        let buffer = project
            .update(cx, |project, cx| project.open_buffer(rules_path, cx))
            .await
            .ok()?;

        let content = buffer
            .read_with(cx, |buffer, _| buffer.text())
            .trim()
            .to_string();

        if content.is_empty() {
            None
        } else {
            Some(content)
        }
    }

    fn build_commit_message_prompt(
        prompt: &str,
        user_agents_md: Option<&str>,
        rules_content: Option<&str>,
        instructions: Option<&str>,
        subject: &str,
        diff_text: &str,
    ) -> String {
        let user_agents_md_section = match user_agents_md {
            Some(user_agents_md) => format!(
                "\n\nThe user has provided the following rules that you should follow when writing the commit message. Project-specific rules may override these instructions when they conflict:\n\
                <rules>\n{user_agents_md}\n</rules>\n"
            ),
            None => String::new(),
        };

        let rules_section = match rules_content {
            Some(rules) => format!(
                "\n\nThe user has provided the following rules specific to this project that you should follow when writing the commit message:\n\
                <project_rules>\n{rules}\n</project_rules>\n"
            ),
            None => String::new(),
        };

        let instructions_section = match instructions {
            Some(instructions) if !instructions.trim().is_empty() => format!(
                "\n\nThe user has provided the following instructions for writing commit messages that you should follow:\n\
                <commit_message_instructions>\n{instructions}\n</commit_message_instructions>\n"
            ),
            _ => String::new(),
        };

        let subject_section = if subject.trim().is_empty() {
            String::new()
        } else {
            format!("\nHere is the user's subject line:\n{subject}")
        };

        format!(
            "{prompt}{user_agents_md_section}{rules_section}{instructions_section}{subject_section}\nHere are the changes in this commit:\n{diff_text}"
        )
    }

    /// Generates a commit message using an LLM.
    pub fn generate_commit_message(&mut self, cx: &mut Context<Self>) {
        if !self.can_commit() || !AgentSettings::get_global(cx).enabled(cx) {
            return;
        }

        let Some(ConfiguredModel { provider, model }) =
            LanguageModelRegistry::read_global(cx).commit_message_model(cx)
        else {
            return;
        };

        let Some(repo) = self.active_repository.as_ref() else {
            return;
        };

        telemetry::event!("Git Commit Message Generated");

        let diff = repo.update(cx, |repo, cx| {
            if self.has_staged_changes() {
                repo.diff(DiffType::HeadToIndex, cx)
            } else {
                repo.diff(DiffType::HeadToWorktree, cx)
            }
        });

        let temperature = AgentSettings::temperature_for_model(&model, cx);

        let include_project_rules =
            AgentSettings::get_global(cx).commit_message_include_project_rules;

        let instructions = AgentSettings::get_global(cx)
            .commit_message_instructions
            .clone();
        let project = self.project.clone();
        let repo_work_dir = repo.read(cx).work_directory_abs_path.clone();

        self.generate_commit_message_task = Some(cx.spawn(async move |this, mut cx| {
            async move {
                let _defer = cx.on_drop(&this, |this, _cx| {
                    this.generate_commit_message_task.take();
                });

                if let Some(task) = cx.update(|cx| {
                    if !provider.is_authenticated(cx) {
                        Some(provider.authenticate(cx))
                    } else {
                        None
                    }
                }) {
                    task.await.log_err();
                }

                let mut diff_text = match diff.await {
                    Ok(result) => match result {
                        Ok(text) => text,
                        Err(e) => {
                            Self::show_commit_message_error(&this, &e, cx);
                            return anyhow::Ok(());
                        }
                    },
                    Err(e) => {
                        Self::show_commit_message_error(&this, &e, cx);
                        return anyhow::Ok(());
                    }
                };

                const MAX_DIFF_BYTES: usize = 20_000;
                diff_text = Self::compress_commit_diff(&diff_text, MAX_DIFF_BYTES);

                let rules_content = if include_project_rules {
                    Self::load_project_rules(&project, &repo_work_dir, &mut cx).await
                } else {
                    None
                };
                let user_agents_md = if include_project_rules {
                    cx.update(|cx| {
                        UserAgentsMd::global(cx)
                            .and_then(|user_agents_md| user_agents_md.content().cloned())
                    })
                } else {
                    None
                };

                let prompt = include_str!("../../src/commit_message_prompt.txt");

                let subject = this.update(cx, |this, cx| {
                    this.commit_editor
                        .read(cx)
                        .text(cx)
                        .lines()
                        .next()
                        .map(ToOwned::to_owned)
                        .unwrap_or_default()
                })?;

                let text_empty = subject.trim().is_empty();

                let content = Self::build_commit_message_prompt(
                    &prompt,
                    user_agents_md.as_deref(),
                    rules_content.as_deref(),
                    instructions.as_deref(),
                    &subject,
                    &diff_text,
                );

                let request = LanguageModelRequest {
                    thread_id: None,
                    prompt_id: None,
                    intent: Some(CompletionIntent::GenerateGitCommitMessage),
                    messages: vec![LanguageModelRequestMessage {
                        role: Role::User,
                        content: vec![content.into()],
                        cache: false,
                        reasoning_details: None,
                    }],
                    tools: Vec::new(),
                    tool_choice: None,
                    stop: Vec::new(),
                    temperature,
                    thinking_allowed: false,
                    thinking_effort: None,
                    speed: None,
                    compact_at_tokens: None,
                };

                let stream = model.stream_completion_text(request, cx);
                match stream.await {
                    Ok(mut messages) => {
                        if !text_empty {
                            this.update(cx, |this, cx| {
                                this.commit_message_buffer(cx).update(cx, |buffer, cx| {
                                    let insert_position = buffer.anchor_before(buffer.len());
                                    buffer.edit(
                                        [(insert_position..insert_position, "\n")],
                                        None,
                                        cx,
                                    )
                                });
                            })?;
                        }

                        while let Some(message) = messages.stream.next().await {
                            match message {
                                Ok(text) => {
                                    this.update(cx, |this, cx| {
                                        this.commit_message_buffer(cx).update(cx, |buffer, cx| {
                                            let insert_position =
                                                buffer.anchor_before(buffer.len());
                                            buffer.edit(
                                                [(insert_position..insert_position, text)],
                                                None,
                                                cx,
                                            );
                                        });
                                    })?;
                                }
                                Err(e) => {
                                    Self::show_commit_message_error(&this, &e, cx);
                                    break;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        Self::show_commit_message_error(&this, &e, cx);
                    }
                }

                anyhow::Ok(())
            }
            .log_err()
            .await
        }));
    }
}
