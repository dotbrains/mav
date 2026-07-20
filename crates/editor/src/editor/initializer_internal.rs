use super::*;

impl Editor {
    pub(super) fn new_internal(
        mode: EditorMode,
        multi_buffer: Entity<MultiBuffer>,
        project: Option<Entity<Project>>,
        display_map: Option<Entity<DisplayMap>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        debug_assert!(
            display_map.is_none() || mode.is_minimap(),
            "Providing a display map for a new editor is only intended for the minimap and might have unintended side effects otherwise!"
        );

        let full_mode = mode.is_full();
        let is_minimap = mode.is_minimap();
        let diagnostics_max_severity = if full_mode {
            EditorSettings::get_global(cx)
                .diagnostics_max_severity
                .unwrap_or(DiagnosticSeverity::Hint)
        } else {
            DiagnosticSeverity::Off
        };
        let display_map = Self::initial_display_map(
            &multi_buffer,
            display_map,
            diagnostics_max_severity,
            window,
            cx,
        );

        let selections = SelectionsCollection::new();
        let initial_focus_state = Self::initial_focus_state(is_minimap, window, cx);
        let soft_wrap_mode_override =
            matches!(mode, EditorMode::SingleLine).then(|| language_settings::SoftWrap::None);
        let project_subscriptions =
            Self::project_subscriptions(full_mode, project.as_ref(), window, cx);
        let buffer_snapshot = multi_buffer.read(cx).snapshot(cx);
        let inlay_hint_settings =
            inlay_hint_settings(selections.newest_anchor().head(), &buffer_snapshot, cx);
        let show_indent_guides =
            if matches!(mode, EditorMode::SingleLine | EditorMode::Minimap { .. }) {
                Some(false)
            } else {
                None
            };
        let initial_project_handles =
            Self::initial_project_handles(&mode, &project, &multi_buffer, cx);

        let editor = Self {
            focus_handle: initial_focus_state.focus_handle,
            show_cursor_when_unfocused: false,
            last_focused_descendant: None,
            buffer: multi_buffer.clone(),
            display_map: display_map.clone(),
            placeholder_display_map: None,
            selections,
            scroll_manager: ScrollManager::new(cx),
            columnar_selection_state: None,
            add_selections_state: None,
            select_next_state: None,
            select_prev_state: None,
            selection_history: SelectionHistory::default(),
            defer_selection_effects: false,
            deferred_selection_effects_state: None,
            autoclose_regions: Vec::new(),
            snippet_stack: InvalidationStack::default(),
            select_syntax_node_history: SelectSyntaxNodeHistory::default(),
            ime_transaction: None,
            active_diagnostics: ActiveDiagnostic::None,
            show_inline_diagnostics: ProjectSettings::get_global(cx).diagnostics.inline.enabled,
            inline_diagnostics_update: Task::ready(()),
            inline_diagnostics: Vec::new(),
            soft_wrap_mode_override,
            diagnostics_max_severity,
            hard_wrap: None,
            completion_provider: project.clone().map(|project| Rc::new(project) as _),
            semantics_provider: project
                .as_ref()
                .map(|project| Rc::new(project.downgrade()) as _),
            collaboration_hub: project.clone().map(|project| Box::new(project) as _),
            project,
            blink_manager: initial_focus_state.blink_manager.clone(),
            show_local_selections: true,
            show_scrollbars: ScrollbarAxes {
                horizontal: full_mode,
                vertical: full_mode,
            },
            minimap_visibility: MinimapVisibility::for_mode(&mode, cx),
            offset_content: !matches!(mode, EditorMode::SingleLine),
            breadcrumbs_visibility: BreadcrumbsVisibility::from_settings(cx),
            show_gutter: full_mode,
            show_line_numbers: (!full_mode).then_some(false),
            use_relative_line_numbers: None,
            disable_expand_excerpt_buttons: !full_mode,
            delegate_expand_excerpts: false,
            delegate_stage_and_restore: false,
            delegate_open_excerpts: false,
            enable_lsp_data: full_mode,
            needs_initial_data_update: full_mode,
            enable_runnables: full_mode,
            enable_code_lens: full_mode,
            enable_mouse_wheel_zoom: full_mode,
            show_git_diff_gutter: None,
            show_code_actions: None,
            show_runnables: None,
            show_bookmarks: None,
            show_breakpoints: None,
            show_diff_review_button: false,
            show_wrap_guides: None,
            show_indent_guides,
            buffers_with_disabled_indent_guides: HashSet::default(),
            highlight_order: 0,
            highlighted_rows: Default::default(),
            background_highlights: HashMap::default(),
            navigation_overlays: HashMap::default(),
            gutter_highlights: Default::default(),
            scrollbar_marker_state: ScrollbarMarkerState::default(),
            active_indent_guides_state: ActiveIndentGuidesState::default(),
            nav_history: None,
            context_menu: RefCell::new(None),
            context_menu_options: None,
            mouse_context_menu: None,
            completion_tasks: Vec::new(),
            inline_blame_popover: None,
            inline_blame_popover_show_task: None,
            signature_help_state: SignatureHelpState::default(),
            auto_signature_help: None,
            find_all_references_task_sources: Vec::new(),
            next_completion_id: 0,
            next_inlay_id: 0,
            code_action_providers: initial_project_handles.code_action_providers,
            code_actions_for_selection: CodeActionsForSelection::None,
            runnables_for_selection_toggle: Task::ready(()),
            quick_selection_highlight_task: None,
            debounced_selection_highlight_task: None,
            debounced_selection_highlight_complete: false,
            last_selection_from_search: false,
            document_highlights_task: None,
            linked_editing_range_task: None,
            pending_rename: None,
            searchable: !is_minimap,
            cursor_shape: EditorSettings::get_global(cx)
                .cursor_shape
                .unwrap_or_default(),
            cursor_offset_on_selection: false,
            current_line_highlight: None,
            autoindent_mode: Some(AutoindentMode::EachLine),
            collapse_matches: false,
            workspace: None,
            input_enabled: !is_minimap,
            expects_character_input: !is_minimap,
            use_modal_editing: full_mode,
            read_only: is_minimap,
            use_autoclose: true,
            use_auto_surround: true,
            use_selection_highlight: true,
            auto_replace_emoji_shortcode: false,
            jsx_tag_auto_close_enabled_in_any_buffer: false,
            leader_id: None,
            remote_id: None,
            hover_state: HoverState::default(),
            pending_mouse_down: None,
            prev_pressure_stage: None,
            hovered_link_state: None,
            edit_prediction_provider: None,
            active_edit_prediction: None,
            stale_edit_prediction_in_menu: None,
            edit_prediction_preview: EditPredictionPreview::Inactive {
                released_too_fast: false,
            },
            inline_diagnostics_enabled: full_mode,
            diagnostics_enabled: full_mode,
            word_completions_enabled: full_mode,
            inline_value_cache: InlineValueCache::new(inlay_hint_settings.show_value_hints),
            gutter_hovered: false,
            pixel_position_of_newest_cursor: None,
            last_bounds: None,
            last_position_map: None,
            last_right_margin: Pixels::ZERO,
            last_horizontal_scrollbar_visible: false,
            expect_bounds_change: None,
            gutter_dimensions: GutterDimensions::default(),
            style: None,
            show_cursor_names: false,
            hovered_cursors: HashMap::default(),
            next_editor_action_id: EditorActionId::default(),
            editor_actions: Rc::default(),
            edit_predictions_hidden_for_vim_mode: false,
            show_edit_predictions_override: None,
            show_completions_on_input_override: None,
            menu_edit_predictions_policy: MenuEditPredictionsPolicy::ByProvider,
            edit_prediction_settings: EditPredictionSettings::Disabled,
            in_leading_whitespace: false,
            custom_context_menu: None,
            show_git_blame_gutter: false,
            show_git_blame_inline: false,
            show_selection_menu: None,
            show_git_blame_inline_delay_task: None,
            git_blame_inline_enabled: full_mode
                && ProjectSettings::get_global(cx).git.inline_blame.enabled,
            render_diff_hunk_controls: Arc::new(render_diff_hunk_controls),
            buffer_serialization: is_minimap.not().then(|| {
                BufferSerialization::new(
                    ProjectSettings::get_global(cx)
                        .session
                        .restore_unsaved_buffers,
                )
            }),
            blame: None,
            blame_subscription: None,
            bookmark_store: initial_project_handles.bookmark_store,
            breakpoint_store: initial_project_handles.breakpoint_store,
            gutter_hover_button: (None, None),
            gutter_diff_review_indicator: (None, None),
            diff_review_drag_state: None,
            diff_review_overlays: Vec::new(),
            stored_review_comments: Vec::new(),
            next_review_comment_id: 0,
            hovered_diff_hunk_row: None,
            _subscriptions: Self::base_subscriptions(
                is_minimap,
                &multi_buffer,
                &display_map,
                &initial_focus_state.blink_manager,
                window,
                cx,
            ),
            runnables: RunnableData::new(),
            pull_diagnostics_task: Task::ready(()),
            colors: None,
            code_lens: None,
            refresh_colors_task: Task::ready(()),
            refresh_code_lens_task: Task::ready(()),
            use_document_folding_ranges: false,
            refresh_folding_ranges_task: Task::ready(()),
            inlay_hints: None,
            next_color_inlay_id: 0,
            post_scroll_update: Task::ready(()),
            linked_edit_ranges: Default::default(),
            in_project_search: false,
            previous_search_ranges: None,
            breadcrumb_header: None,
            focused_block: None,
            next_scroll_position: NextScrollCursorCenterTopBottom::default(),
            addons: Default::default(),
            registered_buffers: HashMap::default(),
            _scroll_cursor_center_top_bottom_task: Task::ready(()),
            selection_mark_mode: false,
            toggle_fold_multiple_buffers: Task::ready(()),
            serialize_selections: Task::ready(()),
            serialize_folds: Task::ready(()),
            text_style_refinement: None,
            load_diff_task: initial_project_handles.load_uncommitted_diff,
            temporary_diff_override: false,
            render_diff_hunks_as_unstaged: false,
            minimap: None,
            change_list: ChangeList::new(),
            mode,
            selection_drag_state: SelectionDragState::None,
            folding_newlines: Task::ready(()),
            lookup_key: None,
            select_next_is_case_sensitive: None,
            on_local_selections_changed: None,
            suppress_selection_callback: false,
            applicable_language_settings: HashMap::default(),
            semantic_token_state: SemanticTokenState::new(cx, full_mode),
            accent_data: None,
            bracket_fetched_tree_sitter_chunks: HashMap::default(),
            number_deleted_lines: false,
            refresh_matching_bracket_highlights_task: Task::ready(()),
            refresh_document_symbols_task: Task::ready(()).shared(),
            lsp_document_links: LspDocumentLinks::new(cx),
            lsp_document_symbols: HashMap::default(),
            refresh_outline_symbols_at_cursor_at_cursor_task: Task::ready(()),
            outline_symbols_at_cursor: None,
            sticky_headers_task: Task::ready(()),
            sticky_headers: None,
            colorize_brackets_task: Task::ready(()),
        };

        Self::finish_initialization(
            editor,
            full_mode,
            is_minimap,
            &multi_buffer,
            project_subscriptions,
            inlay_hint_settings,
            window,
            cx,
        )
    }
}
