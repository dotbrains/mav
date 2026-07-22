use super::*;

impl CompletionsMenu {
    pub fn new(
        id: CompletionId,
        source: CompletionsMenuSource,
        sort_completions: bool,
        show_completion_documentation: bool,
        initial_position: Anchor,
        initial_query: Option<Arc<String>>,
        is_incomplete: bool,
        buffer: Entity<Buffer>,
        completions: Box<[Completion]>,
        scroll_handle: Option<UniformListScrollHandle>,
        display_options: CompletionDisplayOptions,
        snippet_sort_order: SnippetSortOrder,
        language_registry: Option<Arc<LanguageRegistry>>,
        language: Option<LanguageName>,
        cx: &mut Context<Editor>,
    ) -> Self {
        let match_candidates = completions
            .iter()
            .enumerate()
            .map(|(id, completion)| StringMatchCandidate::new(id, completion.label.filter_text()))
            .into_group_map_by(|candidate| completions[candidate.id].match_start)
            .into_iter()
            .collect();

        let completions_menu = Self {
            id,
            source,
            sort_completions,
            initial_position,
            initial_query,
            is_incomplete,
            buffer,
            show_completion_documentation,
            completions: RefCell::new(completions).into(),
            match_candidates,
            entries: Rc::new(RefCell::new(Box::new([]))),
            selected_item: 0,
            filter_task: Task::ready(()),
            cancel_filter: Arc::new(AtomicBool::new(false)),
            scroll_handle: scroll_handle.unwrap_or_else(UniformListScrollHandle::new),
            scroll_handle_aside: ScrollHandle::new(),
            resolve_completions: true,
            last_rendered_range: RefCell::new(None).into(),
            markdown_cache: RefCell::new(VecDeque::new()).into(),
            language_registry,
            language,
            display_options,
            snippet_sort_order,
        };

        completions_menu.start_markdown_parse_for_nearby_entries(cx);

        completions_menu
    }

    pub fn new_snippet_choices(
        id: CompletionId,
        sort_completions: bool,
        choices: &Vec<String>,
        initial_position: Anchor,
        selection: Range<text::Anchor>,
        buffer: Entity<Buffer>,
        scroll_handle: Option<UniformListScrollHandle>,
        snippet_sort_order: SnippetSortOrder,
    ) -> Self {
        let completions = choices
            .iter()
            .map(|choice| Completion {
                replace_range: selection.clone(),
                new_text: choice.to_string(),
                label: CodeLabel::plain(choice.to_string(), None),
                match_start: None,
                snippet_deduplication_key: None,
                icon_path: None,
                icon_color: None,
                documentation: None,
                confirm: None,
                insert_text_mode: None,
                source: CompletionSource::Custom,
                group: None,
            })
            .collect();

        let match_candidates = Arc::new([(
            None,
            choices
                .iter()
                .enumerate()
                .map(|(id, completion)| StringMatchCandidate::new(id, completion))
                .collect(),
        )]);
        let entries = choices
            .iter()
            .enumerate()
            .map(|(id, completion)| {
                CompletionMenuEntry::Match(StringMatch {
                    candidate_id: id,
                    score: 1.,
                    positions: vec![],
                    string: completion.clone(),
                })
            })
            .collect();
        Self {
            id,
            source: CompletionsMenuSource::SnippetChoices,
            sort_completions,
            initial_position,
            initial_query: None,
            is_incomplete: false,
            buffer,
            completions: RefCell::new(completions).into(),
            match_candidates,
            entries: RefCell::new(entries).into(),
            selected_item: 0,
            filter_task: Task::ready(()),
            cancel_filter: Arc::new(AtomicBool::new(false)),
            scroll_handle: scroll_handle.unwrap_or_else(UniformListScrollHandle::new),
            scroll_handle_aside: ScrollHandle::new(),
            resolve_completions: false,
            show_completion_documentation: false,
            last_rendered_range: RefCell::new(None).into(),
            markdown_cache: RefCell::new(VecDeque::new()).into(),
            language_registry: None,
            language: None,
            display_options: CompletionDisplayOptions::default(),
            snippet_sort_order,
        }
    }
}
