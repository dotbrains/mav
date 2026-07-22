use super::*;

impl<T: PromptCompletionProviderDelegate> CompletionProvider for PromptCompletionProvider<T> {
    fn completions(
        &self,
        buffer: &Entity<Buffer>,
        buffer_position: Anchor,
        _trigger: CompletionContext,
        window: &mut Window,
        cx: &mut Context<Editor>,
    ) -> Task<Result<Vec<CompletionResponse>>> {
        let state = buffer.update(cx, |buffer, cx| {
            let position = buffer_position.to_point(buffer);
            let line_start = Point::new(position.row, 0);
            let offset_to_line = buffer.point_to_offset(line_start);
            let mut lines = buffer.text_for_range(line_start..position).lines();
            let line = lines.next()?;
            PromptCompletion::try_parse(line, offset_to_line, &self.source.supported_modes(cx))
        });
        let Some(state) = state else {
            return Task::ready(Ok(Vec::new()));
        };

        let Some(workspace) = self.workspace.upgrade() else {
            return Task::ready(Ok(Vec::new()));
        };

        let project = workspace.read(cx).project().clone();
        let snapshot = buffer.read(cx).snapshot();
        let source_range = snapshot.anchor_before(state.source_range().start)
            ..snapshot.anchor_after(state.source_range().end);

        let source = self.source.clone();
        let editor = self.editor.clone();
        let mention_set = self.mention_set.downgrade();
        match state {
            PromptCompletion::SlashCommand(SlashCommandCompletion {
                command, argument, ..
            }) => {
                let search_task = self.search_slash_commands(command.unwrap_or_default(), cx);
                // Keep the category section headers visible while the user is
                // still narrowing the command name (`/c`); only drop them once
                // they've moved on to typing the command's argument, where
                // grouping no longer applies.
                let show_section_headers = argument.is_none();

                let source_highlight_id = cx
                    .theme()
                    .syntax()
                    .highlight_id("variable")
                    .map(HighlightId::new);

                type SkillInfo = (
                    String,
                    SharedString,
                    Option<Hsla>,
                    Arc<dyn Fn(CompletionIntent, &mut Window, &mut App) -> bool + Send + Sync>,
                );
                let slash_candidates: Task<Vec<(SlashCompletionCandidate, Option<SkillInfo>)>> = {
                    let source = source.clone();
                    cx.spawn(async move |_this, cx| {
                        let candidates = search_task.await;
                        cx.update(|cx| {
                            candidates
                                .into_iter()
                                .map(|candidate| match &candidate {
                                    SlashCompletionCandidate::Skill(skill) => {
                                        let uri = MentionUri::Skill {
                                            name: skill.name.to_string(),
                                            source: skill.source.to_string(),
                                            skill_file_path: skill.skill_file_path.clone(),
                                        };
                                        let new_text = format!("{} ", uri.as_link());
                                        let new_text_len = new_text.len();
                                        let icon_path = skill_completion_icon_path(skill, &uri, cx);
                                        let icon_color = skill_completion_icon_color(skill, cx);
                                        let crease_text: SharedString = uri.name().into();
                                        let confirm = confirm_completion_callback(
                                            crease_text,
                                            source_range.start,
                                            new_text_len - 1,
                                            uri,
                                            source.clone(),
                                            editor.clone(),
                                            mention_set.clone(),
                                            workspace.clone(),
                                        );
                                        (
                                            candidate,
                                            Some((new_text, icon_path, icon_color, confirm)),
                                        )
                                    }
                                    SlashCompletionCandidate::Command(_) => (candidate, None),
                                })
                                .collect::<Vec<(SlashCompletionCandidate, Option<SkillInfo>)>>()
                        })
                    })
                };

                cx.background_spawn(async move {
                    let mut slash_candidates = slash_candidates.await;
                    // `slash_candidates` arrives in fuzzy-match order (best
                    // first). Keep each group's items contiguous so section
                    // headers render once, but order the groups by their
                    // best-scoring member. That way an exact/prefix match (e.g.
                    // `/compa` -> `compact`) floats its whole section to the top
                    // and becomes the default selection, instead of being
                    // buried under a less relevant skill. Within a group, the
                    // fuzzy-match order is preserved (the sort is stable).
                    group_by_relevance(&mut slash_candidates, |(candidate, _)| {
                        slash_completion_group_key(candidate)
                    });
                    let completions = slash_candidates
                        .into_iter()
                        .map(|(candidate, skill_info)| match candidate {
                            SlashCompletionCandidate::Skill(skill) => {
                                let label = build_slash_item_label(
                                    &skill.name,
                                    Some(&skill.source),
                                    source_highlight_id,
                                );
                                let Some((new_text, icon_path, icon_color, confirm)) = skill_info
                                else {
                                    unreachable!("skill candidates always have confirm callbacks")
                                };
                                Completion {
                                    replace_range: source_range.clone(),
                                    new_text,
                                    label,
                                    documentation: Some(skill_completion_documentation(&skill)),
                                    source: project::CompletionSource::Custom,
                                    icon_path: Some(icon_path),
                                    icon_color,
                                    match_start: None,
                                    snippet_deduplication_key: None,
                                    insert_text_mode: None,
                                    confirm: Some(confirm),
                                    group: show_section_headers.then(|| CompletionGroup {
                                        key: "skills".into(),
                                        label: Some("Skills".into()),
                                    }),
                                }
                            }
                            SlashCompletionCandidate::Command(command) => {
                                let label =
                                    build_slash_command_label(&command, source_highlight_id);
                                let new_text = match (command.source.as_ref(), argument.as_ref()) {
                                    (Some(source), Some(argument)) => {
                                        format!("/{}:{} {}", source, command.name, argument)
                                    }
                                    (Some(source), None) => {
                                        format!("/{}:{} ", source, command.name)
                                    }
                                    (None, Some(argument)) => {
                                        format!("/{} {}", command.name, argument)
                                    }
                                    (None, None) => format!("/{} ", command.name),
                                };

                                let is_missing_argument =
                                    command.requires_argument && argument.is_none();
                                let group = show_section_headers.then(|| command.group());

                                let icon_path = (command.category
                                    == Some(acp_thread::CommandCategory::Native)
                                    && command.name.as_ref() == agent::COMPACT_COMMAND_NAME)
                                    .then(|| IconName::Compact.path().into());

                                Completion {
                                    replace_range: source_range.clone(),
                                    new_text,
                                    label,
                                    documentation: Some(
                                        CompletionDocumentation::MultiLinePlainText(
                                            command.description.into(),
                                        ),
                                    ),
                                    source: project::CompletionSource::Custom,
                                    icon_path,
                                    icon_color: None,
                                    match_start: None,
                                    snippet_deduplication_key: None,
                                    insert_text_mode: None,
                                    confirm: Some(Arc::new({
                                        let source = source.clone();
                                        move |intent, _window, cx| {
                                            if !is_missing_argument {
                                                cx.defer({
                                                    let source = source.clone();
                                                    move |cx| match intent {
                                                        CompletionIntent::Complete
                                                        | CompletionIntent::CompleteWithInsert
                                                        | CompletionIntent::CompleteWithReplace => {
                                                            source.confirm_command(cx);
                                                        }
                                                        CompletionIntent::Compose => {}
                                                    }
                                                });
                                            }
                                            false
                                        }
                                    })),
                                    group,
                                }
                            }
                        })
                        .collect();

                    Ok(vec![CompletionResponse {
                        completions,
                        display_options: CompletionDisplayOptions {
                            dynamic_width: true,
                        },
                        is_incomplete: true,
                    }])
                })
            }
            PromptCompletion::Mention(MentionCompletion { mode, argument, .. }) => {
                if let Some(PromptContextType::Diagnostics) = mode {
                    if argument.is_some() {
                        return Task::ready(Ok(Vec::new()));
                    }

                    let completions = Self::completion_for_diagnostics(
                        source_range.clone(),
                        source.clone(),
                        editor.clone(),
                        mention_set.clone(),
                        workspace.clone(),
                        cx,
                    );
                    if !completions.is_empty() {
                        return Task::ready(Ok(vec![CompletionResponse {
                            completions,
                            display_options: CompletionDisplayOptions::default(),
                            is_incomplete: false,
                        }]));
                    }
                }

                let show_section_headers = mode.is_none() && argument.is_none();
                let query = argument.unwrap_or_default();
                let search_task =
                    self.search_mentions(mode, query, Arc::<AtomicBool>::default(), cx);

                // Calculate maximum characters available for the full label (file_name + space + directory)
                // based on maximum menu width after accounting for padding, spacing, and icon width
                let label_max_chars = {
                    // Base06 left padding + Base06 gap + Base06 right padding + icon width
                    let used_pixels = DynamicSpacing::Base06.px(cx) * 3.0
                        + IconSize::XSmall.rems() * window.rem_size();

                    let style = window.text_style();
                    let font_id = window.text_system().resolve_font(&style.font());
                    let font_size = TextSize::Small.rems(cx).to_pixels(window.rem_size());

                    // Fallback em_width of 10px matches file_finder.rs fallback for TextSize::Small
                    let em_width = cx
                        .text_system()
                        .em_width(font_id, font_size)
                        .unwrap_or(px(10.0));

                    // Calculate available pixels for text (file_name + directory)
                    // Using max width since dynamic_width allows the menu to expand up to this
                    let available_pixels = COMPLETION_MENU_MAX_WIDTH - used_pixels;

                    // Convert to character count (total available for file_name + directory)
                    (f32::from(available_pixels) / f32::from(em_width)) as usize
                };

                cx.spawn(async move |_, cx| {
                    let mut matches = search_task.await;
                    if show_section_headers {
                        matches.sort_by_key(|mat| match mat {
                            Match::File(FileMatch {
                                is_recent: true, ..
                            })
                            | Match::RecentThread(_) => 0,
                            Match::Entry(_) | Match::BranchDiff(_) => 1,
                            _ => 2,
                        });
                    }

                    let completions = cx.update(|cx| {
                        matches
                            .into_iter()
                            .filter_map(|mat| {
                                let group = if show_section_headers {
                                    match &mat {
                                        Match::File(FileMatch {
                                            is_recent: true, ..
                                        })
                                        | Match::RecentThread(_) => Some(CompletionGroup {
                                            key: "recent".into(),
                                            label: None,
                                        }),
                                        Match::Entry(_) | Match::BranchDiff(_) => {
                                            Some(CompletionGroup {
                                                key: "context".into(),
                                                label: None,
                                            })
                                        }
                                        _ => None,
                                    }
                                } else {
                                    None
                                };
                                let mut completion = match mat {
                                    Match::File(FileMatch { mat, is_recent }) => {
                                        let project_path = ProjectPath {
                                            worktree_id: WorktreeId::from_usize(mat.worktree_id),
                                            path: mat.path.clone(),
                                        };

                                        // If path is empty, this means we're matching with the root directory itself
                                        // so we use the path_prefix as the name
                                        let path_prefix = if mat.path.is_empty() {
                                            project
                                                .read(cx)
                                                .worktree_for_id(project_path.worktree_id, cx)
                                                .map(|wt| wt.read(cx).root_name().into())
                                                .unwrap_or_else(|| mat.path_prefix.clone())
                                        } else {
                                            mat.path_prefix.clone()
                                        };

                                        Self::completion_for_path(
                                            project_path,
                                            &path_prefix,
                                            is_recent,
                                            mat.is_dir,
                                            source_range.clone(),
                                            source.clone(),
                                            editor.clone(),
                                            mention_set.clone(),
                                            workspace.clone(),
                                            project.clone(),
                                            label_max_chars,
                                            cx,
                                        )
                                    }
                                    Match::Symbol(SymbolMatch { symbol, .. }) => {
                                        Self::completion_for_symbol(
                                            symbol,
                                            source_range.clone(),
                                            source.clone(),
                                            editor.clone(),
                                            mention_set.clone(),
                                            workspace.clone(),
                                            label_max_chars,
                                            cx,
                                        )
                                    }
                                    Match::Thread(thread) => Some(Self::completion_for_thread(
                                        thread.session_id,
                                        Some(thread.title),
                                        source_range.clone(),
                                        false,
                                        source.clone(),
                                        editor.clone(),
                                        mention_set.clone(),
                                        workspace.clone(),
                                        cx,
                                    )),
                                    Match::RecentThread(thread) => {
                                        Some(Self::completion_for_thread(
                                            thread.session_id,
                                            Some(thread.title),
                                            source_range.clone(),
                                            true,
                                            source.clone(),
                                            editor.clone(),
                                            mention_set.clone(),
                                            workspace.clone(),
                                            cx,
                                        ))
                                    }
                                    Match::Skill(skill) => Some(Self::completion_for_skill(
                                        skill,
                                        source_range.clone(),
                                        source.clone(),
                                        editor.clone(),
                                        mention_set.clone(),
                                        workspace.clone(),
                                        cx,
                                    )),
                                    Match::Fetch(url) => Self::completion_for_fetch(
                                        source_range.clone(),
                                        url,
                                        source.clone(),
                                        editor.clone(),
                                        mention_set.clone(),
                                        workspace.clone(),
                                        cx,
                                    ),
                                    Match::Entry(EntryMatch { entry, .. }) => {
                                        Self::completion_for_entry(
                                            entry,
                                            source_range.clone(),
                                            editor.clone(),
                                            mention_set.clone(),
                                            &workspace,
                                            cx,
                                        )
                                    }
                                    Match::BranchDiff(branch_diff) => {
                                        Some(Self::build_branch_diff_completion(
                                            branch_diff.base_ref,
                                            source_range.clone(),
                                            source.clone(),
                                            editor.clone(),
                                            mention_set.clone(),
                                            workspace.clone(),
                                            cx,
                                        ))
                                    }
                                };
                                if let Some(completion) = &mut completion {
                                    completion.group = group;
                                }
                                completion
                            })
                            .collect::<Vec<_>>()
                    });

                    Ok(vec![CompletionResponse {
                        completions,
                        display_options: CompletionDisplayOptions {
                            dynamic_width: true,
                        },
                        // Since this does its own filtering (see `filter_completions()` returns false),
                        // there is no benefit to computing whether this set of completions is incomplete.
                        is_incomplete: true,
                    }])
                })
            }
        }
    }

    fn is_completion_trigger(
        &self,
        buffer: &Entity<language::Buffer>,
        position: language::Anchor,
        _text: &str,
        _trigger_in_words: bool,
        cx: &mut Context<Editor>,
    ) -> bool {
        let buffer = buffer.read(cx);
        let position = position.to_point(buffer);
        let line_start = Point::new(position.row, 0);
        let offset_to_line = buffer.point_to_offset(line_start);
        let mut lines = buffer.text_for_range(line_start..position).lines();
        if let Some(line) = lines.next() {
            PromptCompletion::try_parse(line, offset_to_line, &self.source.supported_modes(cx))
                .filter(|completion| {
                    // Right now we don't support completing arguments of slash commands
                    let is_slash_command_with_argument = matches!(
                        completion,
                        PromptCompletion::SlashCommand(SlashCommandCompletion {
                            argument: Some(_),
                            ..
                        })
                    );
                    !is_slash_command_with_argument
                })
                .map(|completion| {
                    completion.source_range().start <= offset_to_line + position.column as usize
                        && completion.source_range().end
                            >= offset_to_line + position.column as usize
                })
                .unwrap_or(false)
        } else {
            false
        }
    }

    fn sort_completions(&self) -> bool {
        false
    }

    fn filter_completions(&self) -> bool {
        false
    }
}
