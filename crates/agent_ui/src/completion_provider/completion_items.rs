use super::*;

impl<T: PromptCompletionProviderDelegate> PromptCompletionProvider<T> {
    fn completion_for_entry(
        entry: PromptContextEntry,
        source_range: Range<Anchor>,
        editor: WeakEntity<Editor>,
        mention_set: WeakEntity<MentionSet>,
        workspace: &Entity<Workspace>,
        cx: &mut App,
    ) -> Option<Completion> {
        match entry {
            PromptContextEntry::Mode(mode) => Some(Completion {
                replace_range: source_range,
                new_text: format!("@{} ", mode.keyword()),
                label: CodeLabel::plain(mode.label().to_string(), None),
                icon_path: Some(mode.icon().path().into()),
                icon_color: None,
                documentation: None,
                source: project::CompletionSource::Custom,
                match_start: None,
                snippet_deduplication_key: None,
                insert_text_mode: None,
                // This ensures that when a user accepts this completion, the
                // completion menu will still be shown after "@category " is
                // inserted
                confirm: Some(Arc::new(|_, _, _| true)),
                group: None,
            }),
            PromptContextEntry::Action(action) => {
                let selection = workspace.update(cx, |workspace, cx| {
                    AgentContextSource::from_active(workspace, cx)?
                        .read_selection(workspace, false, cx)
                });
                Self::completion_for_action(action, source_range, editor, mention_set, selection)
            }
        }
    }

    fn completion_for_thread(
        session_id: acp::SessionId,
        title: Option<SharedString>,
        source_range: Range<Anchor>,
        recent: bool,
        source: Arc<T>,
        editor: WeakEntity<Editor>,
        mention_set: WeakEntity<MentionSet>,
        workspace: Entity<Workspace>,
        cx: &mut App,
    ) -> Completion {
        let title = session_title(title);
        let uri = MentionUri::Thread {
            id: session_id,
            name: title.to_string(),
        };

        let icon_for_completion = if recent {
            IconName::HistoryRerun.path().into()
        } else {
            uri.icon_path(cx)
        };

        let new_text = format!("{} ", uri.as_link());

        let new_text_len = new_text.len();
        Completion {
            replace_range: source_range.clone(),
            new_text,
            label: CodeLabel::plain(title.to_string(), None),
            documentation: None,
            insert_text_mode: None,
            source: project::CompletionSource::Custom,
            match_start: None,
            snippet_deduplication_key: None,
            icon_path: Some(icon_for_completion),
            icon_color: None,
            confirm: Some(confirm_completion_callback(
                title,
                source_range.start,
                new_text_len - 1,
                uri,
                source,
                editor,
                mention_set,
                workspace,
            )),
            group: None,
        }
    }

    fn completion_for_skill(
        skill: AvailableSkill,
        source_range: Range<Anchor>,
        source: Arc<T>,
        editor: WeakEntity<Editor>,
        mention_set: WeakEntity<MentionSet>,
        workspace: Entity<Workspace>,
        cx: &mut App,
    ) -> Completion {
        let uri = MentionUri::Skill {
            name: skill.name.to_string(),
            source: skill.source.to_string(),
            skill_file_path: skill.skill_file_path.clone(),
        };
        let new_text = format!("{} ", uri.as_link());
        let new_text_len = new_text.len();
        let icon_path = skill_completion_icon_path(&skill, &uri, cx);
        let crease_text: SharedString = uri.name().into();
        let source_highlight_id = cx
            .theme()
            .syntax()
            .highlight_id("variable")
            .map(HighlightId::new);
        let label = build_slash_item_label(&skill.name, Some(&skill.source), source_highlight_id);
        Completion {
            replace_range: source_range.clone(),
            new_text,
            label,
            documentation: Some(skill_completion_documentation(&skill)),
            insert_text_mode: None,
            source: project::CompletionSource::Custom,
            match_start: None,
            snippet_deduplication_key: None,
            icon_path: Some(icon_path),
            icon_color: skill_completion_icon_color(&skill, cx),
            confirm: Some(confirm_completion_callback(
                crease_text,
                source_range.start,
                new_text_len - 1,
                uri,
                source,
                editor,
                mention_set,
                workspace,
            )),
            group: None,
        }
    }

    pub(crate) fn completion_for_path(
        project_path: ProjectPath,
        path_prefix: &RelPath,
        is_recent: bool,
        is_directory: bool,
        source_range: Range<Anchor>,
        source: Arc<T>,
        editor: WeakEntity<Editor>,
        mention_set: WeakEntity<MentionSet>,
        workspace: Entity<Workspace>,
        project: Entity<Project>,
        label_max_chars: usize,
        cx: &mut App,
    ) -> Option<Completion> {
        let path_style = project.read(cx).path_style(cx);
        let (file_name, directory) =
            extract_file_name_and_directory(&project_path.path, path_prefix, path_style);

        let label = build_code_label_for_path(
            &file_name,
            directory.as_ref().map(|s| s.as_ref()),
            None,
            label_max_chars,
            cx,
        );

        let abs_path = project.read(cx).absolute_path(&project_path, cx)?;

        let uri = if is_directory {
            MentionUri::Directory { abs_path }
        } else {
            MentionUri::File { abs_path }
        };

        let crease_icon_path = uri.icon_path(cx);
        let completion_icon_path = if is_recent {
            IconName::HistoryRerun.path().into()
        } else {
            crease_icon_path
        };

        let new_text = format!("{} ", uri.as_link());
        let new_text_len = new_text.len();
        Some(Completion {
            replace_range: source_range.clone(),
            new_text,
            label,
            documentation: None,
            source: project::CompletionSource::Custom,
            icon_path: Some(completion_icon_path),
            icon_color: None,
            match_start: None,
            snippet_deduplication_key: None,
            insert_text_mode: None,
            confirm: Some(confirm_completion_callback(
                file_name,
                source_range.start,
                new_text_len - 1,
                uri,
                source,
                editor,
                mention_set,
                workspace,
            )),
            group: None,
        })
    }

    fn completion_for_symbol(
        symbol: Symbol,
        source_range: Range<Anchor>,
        source: Arc<T>,
        editor: WeakEntity<Editor>,
        mention_set: WeakEntity<MentionSet>,
        workspace: Entity<Workspace>,
        label_max_chars: usize,
        cx: &mut App,
    ) -> Option<Completion> {
        let project = workspace.read(cx).project().clone();

        let (abs_path, file_name) = match &symbol.path {
            SymbolLocation::InProject(project_path) => (
                project.read(cx).absolute_path(&project_path, cx)?,
                project_path.path.file_name()?.to_string().into(),
            ),
            SymbolLocation::OutsideProject {
                abs_path,
                signature: _,
            } => (
                PathBuf::from(abs_path.as_ref()),
                abs_path.file_name().map(|f| f.to_string_lossy())?,
            ),
        };

        let label = build_code_label_for_path(
            &symbol.name,
            Some(&file_name),
            Some(symbol.range.start.0.row + 1),
            label_max_chars,
            cx,
        );

        let uri = MentionUri::Symbol {
            abs_path,
            name: symbol.name.clone(),
            line_range: symbol.range.start.0.row..=symbol.range.end.0.row,
        };
        let new_text = format!("{} ", uri.as_link());
        let new_text_len = new_text.len();
        let icon_path = uri.icon_path(cx);
        Some(Completion {
            replace_range: source_range.clone(),
            new_text,
            label,
            documentation: None,
            source: project::CompletionSource::Custom,
            icon_path: Some(icon_path),
            icon_color: None,
            match_start: None,
            snippet_deduplication_key: None,
            insert_text_mode: None,
            confirm: Some(confirm_completion_callback(
                symbol.name.into(),
                source_range.start,
                new_text_len - 1,
                uri,
                source,
                editor,
                mention_set,
                workspace,
            )),
            group: None,
        })
    }

    fn completion_for_fetch(
        source_range: Range<Anchor>,
        url_to_fetch: SharedString,
        source: Arc<T>,
        editor: WeakEntity<Editor>,
        mention_set: WeakEntity<MentionSet>,
        workspace: Entity<Workspace>,
        cx: &mut App,
    ) -> Option<Completion> {
        let new_text = format!("@fetch {} ", url_to_fetch);
        let url_to_fetch = url::Url::parse(url_to_fetch.as_ref())
            .or_else(|_| url::Url::parse(&format!("https://{url_to_fetch}")))
            .ok()?;
        let mention_uri = MentionUri::Fetch {
            url: url_to_fetch.clone(),
        };
        let icon_path = mention_uri.icon_path(cx);
        Some(Completion {
            replace_range: source_range.clone(),
            new_text: new_text.clone(),
            label: CodeLabel::plain(url_to_fetch.to_string(), None),
            documentation: None,
            source: project::CompletionSource::Custom,
            icon_path: Some(icon_path),
            icon_color: None,
            match_start: None,
            snippet_deduplication_key: None,
            insert_text_mode: None,
            confirm: Some(confirm_completion_callback(
                url_to_fetch.to_string().into(),
                source_range.start,
                new_text.len() - 1,
                mention_uri,
                source,
                editor,
                mention_set,
                workspace,
            )),
            group: None,
        })
    }

    pub(crate) fn completion_for_action(
        action: PromptContextAction,
        source_range: Range<Anchor>,
        editor: WeakEntity<Editor>,
        mention_set: WeakEntity<MentionSet>,
        selection: Option<AgentContextSelection>,
    ) -> Option<Completion> {
        let (new_text, on_action) = match action {
            PromptContextAction::AddSelections => match selection? {
                AgentContextSelection::Editor(editor_selections) => {
                    completion_text_for_editor_selections(
                        source_range.clone(),
                        editor,
                        mention_set,
                        editor_selections,
                    )
                }
                AgentContextSelection::Terminal(terminal_selections) => {
                    completion_text_for_terminal_selections(
                        source_range.clone(),
                        editor,
                        mention_set,
                        terminal_selections,
                    )
                }
            },
        };

        Some(Completion {
            replace_range: source_range,
            new_text,
            label: CodeLabel::plain(action.label().to_string(), None),
            icon_path: Some(action.icon().path().into()),
            icon_color: None,
            documentation: None,
            source: project::CompletionSource::Custom,
            match_start: None,
            snippet_deduplication_key: None,
            insert_text_mode: None,
            // This ensures that when a user accepts this completion, the
            // completion menu will still be shown after "@category " is
            // inserted
            confirm: Some(on_action),
            group: None,
        })
    }
}
