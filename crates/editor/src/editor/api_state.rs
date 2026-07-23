use super::*;

impl Editor {
    pub fn display_snapshot(&self, cx: &mut App) -> DisplaySnapshot {
        self.display_map.update(cx, |map, cx| map.snapshot(cx))
    }

    pub fn deploy_mouse_context_menu(
        &mut self,
        position: gpui::Point<Pixels>,
        context_menu: Entity<ContextMenu>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.mouse_context_menu = Some(MouseContextMenu::new(
            self,
            crate::mouse_context_menu::MenuPosition::PinnedToScreen(position),
            context_menu,
            window,
            cx,
        ));
    }

    pub fn mouse_menu_is_focused(&self, window: &Window, cx: &App) -> bool {
        self.mouse_context_menu
            .as_ref()
            .is_some_and(|menu| menu.context_menu.focus_handle(cx).is_focused(window))
    }

    pub fn last_bounds(&self) -> Option<&Bounds<Pixels>> {
        self.last_bounds.as_ref()
    }

    pub(crate) fn last_right_margin(&self) -> Pixels {
        self.last_right_margin
    }

    pub(crate) fn last_horizontal_scrollbar_visible(&self) -> bool {
        self.last_horizontal_scrollbar_visible
    }

    pub fn leader_id(&self) -> Option<CollaboratorId> {
        self.leader_id
    }

    pub fn buffer(&self) -> &Entity<MultiBuffer> {
        &self.buffer
    }

    pub fn project(&self) -> Option<&Entity<Project>> {
        self.project.as_ref()
    }

    pub fn workspace(&self) -> Option<Entity<Workspace>> {
        self.workspace.as_ref()?.0.upgrade()
    }

    /// Detaches a task and shows an error notification in the workspace if available,
    /// otherwise just logs the error.
    pub fn detach_and_notify_err<R, E>(
        &self,
        task: Task<Result<R, E>>,
        window: &mut Window,
        cx: &mut App,
    ) where
        E: std::fmt::Debug + std::fmt::Display + 'static,
        R: 'static,
    {
        if let Some(workspace) = self.workspace() {
            task.detach_and_notify_err(workspace.downgrade(), window, cx);
        } else {
            task.detach_and_log_err(cx);
        }
    }

    /// Returns the workspace serialization ID if this editor should be serialized.
    pub(crate) fn workspace_serialization_id(&self, _cx: &App) -> Option<WorkspaceId> {
        self.workspace
            .as_ref()
            .filter(|_| self.should_serialize_buffer())
            .and_then(|workspace| workspace.1)
    }

    pub fn title<'a>(&self, cx: &'a App) -> Cow<'a, str> {
        self.buffer().read(cx).title(cx)
    }

    pub fn snapshot(&self, window: &Window, cx: &mut App) -> EditorSnapshot {
        let git_blame_gutter_max_author_length = self
            .render_git_blame_gutter(cx)
            .then(|| {
                if let Some(blame) = self.blame.as_ref() {
                    let max_author_length =
                        blame.update(cx, |blame, cx| blame.max_author_length(cx));
                    Some(max_author_length)
                } else {
                    None
                }
            })
            .flatten();

        let display_snapshot = self.display_map.update(cx, |map, cx| map.snapshot(cx));

        EditorSnapshot {
            mode: self.mode.clone(),
            show_gutter: self.show_gutter,
            offset_content: self.offset_content,
            show_line_numbers: self.show_line_numbers,
            number_deleted_lines: self.number_deleted_lines,
            show_git_diff_gutter: self.show_git_diff_gutter,
            semantic_tokens_enabled: self.semantic_token_state.enabled(),
            show_code_actions: self.show_code_actions,
            show_runnables: self.show_runnables,
            show_bookmarks: self.show_bookmarks,
            show_breakpoints: self.show_breakpoints,
            git_blame_gutter_max_author_length,
            scroll_anchor: self.scroll_manager.shared_scroll_anchor(cx),
            display_snapshot,
            placeholder_display_snapshot: self
                .placeholder_display_map
                .as_ref()
                .map(|display_map| display_map.update(cx, |map, cx| map.snapshot(cx))),
            ongoing_scroll: self.scroll_manager.ongoing_scroll(),
            is_focused: self.focus_handle.is_focused(window),
            current_line_highlight: self
                .current_line_highlight
                .unwrap_or_else(|| EditorSettings::get_global(cx).current_line_highlight),
            gutter_hovered: self.gutter_hovered,
        }
    }

    pub fn language_at<T: ToOffset>(&self, point: T, cx: &App) -> Option<Arc<Language>> {
        self.buffer.read(cx).language_at(point, cx)
    }

    pub fn file_at<T: ToOffset>(&self, point: T, cx: &App) -> Option<Arc<dyn language::File>> {
        self.buffer.read(cx).read(cx).file_at(point).cloned()
    }

    pub fn active_buffer(&self, cx: &App) -> Option<Entity<Buffer>> {
        let multibuffer = self.buffer.read(cx);
        let snapshot = multibuffer.snapshot(cx);
        let (anchor, _) =
            snapshot.anchor_to_buffer_anchor(self.selections.newest_anchor().head())?;
        multibuffer.buffer(anchor.buffer_id)
    }

    pub fn mode(&self) -> &EditorMode {
        &self.mode
    }

    pub fn set_mode(&mut self, mode: EditorMode) {
        self.mode = mode;
    }

    pub fn collaboration_hub(&self) -> Option<&dyn CollaborationHub> {
        self.collaboration_hub.as_deref()
    }

    pub fn set_collaboration_hub(&mut self, hub: Box<dyn CollaborationHub>) {
        self.collaboration_hub = Some(hub);
    }

    pub fn set_in_project_search(&mut self, in_project_search: bool) {
        self.in_project_search = in_project_search;
    }

    pub fn set_custom_context_menu(
        &mut self,
        f: impl 'static
        + Fn(
            &mut Self,
            DisplayPoint,
            &mut Window,
            &mut Context<Self>,
        ) -> Option<Entity<ui::ContextMenu>>,
    ) {
        self.custom_context_menu = Some(Box::new(f))
    }

    pub fn semantics_provider(&self) -> Option<Rc<dyn SemanticsProvider>> {
        self.semantics_provider.clone()
    }

    pub fn set_semantics_provider(&mut self, provider: Option<Rc<dyn SemanticsProvider>>) {
        self.semantics_provider = provider;
    }

    pub fn placeholder_text(&self, cx: &mut App) -> Option<String> {
        self.placeholder_display_map
            .as_ref()
            .map(|display_map| display_map.update(cx, |map, cx| map.snapshot(cx)).text())
    }

    pub fn set_placeholder_text(
        &mut self,
        placeholder_text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let multibuffer = cx
            .new(|cx| MultiBuffer::singleton(cx.new(|cx| Buffer::local(placeholder_text, cx)), cx));

        let style = window.text_style();

        self.placeholder_display_map = Some(cx.new(|cx| {
            DisplayMap::new(
                multibuffer,
                style.font(),
                style.font_size.to_pixels(window.rem_size()),
                None,
                FILE_HEADER_HEIGHT,
                MULTI_BUFFER_EXCERPT_HEADER_HEIGHT,
                Default::default(),
                DiagnosticSeverity::Off,
                cx,
            )
        }));
        cx.notify();
    }

    pub fn set_cursor_shape(&mut self, cursor_shape: CursorShape, cx: &mut Context<Self>) {
        self.cursor_shape = cursor_shape;

        // Disrupt blink for immediate user feedback that the cursor shape has changed
        self.blink_manager.update(cx, BlinkManager::show_cursor);

        cx.notify();
    }

    pub fn show_cursor(&mut self, cx: &mut Context<Self>) {
        self.blink_manager.update(cx, BlinkManager::show_cursor);
    }

    pub fn cursor_shape(&self) -> CursorShape {
        self.cursor_shape
    }

    pub fn set_cursor_offset_on_selection(&mut self, set_cursor_offset_on_selection: bool) {
        self.cursor_offset_on_selection = set_cursor_offset_on_selection;
    }

    pub fn set_current_line_highlight(
        &mut self,
        current_line_highlight: Option<CurrentLineHighlight>,
    ) {
        self.current_line_highlight = current_line_highlight;
    }

    pub fn set_collapse_matches(&mut self, collapse_matches: bool) {
        self.collapse_matches = collapse_matches;
    }

    pub fn range_for_match<T: std::marker::Copy>(&self, range: &Range<T>) -> Range<T> {
        if self.collapse_matches {
            return range.start..range.start;
        }
        range.clone()
    }

    pub fn clip_at_line_ends(&mut self, cx: &mut Context<Self>) -> bool {
        self.display_map.read(cx).clip_at_line_ends
    }

    pub fn set_clip_at_line_ends(&mut self, clip: bool, cx: &mut Context<Self>) {
        if self.display_map.read(cx).clip_at_line_ends != clip {
            self.display_map
                .update(cx, |map, _| map.clip_at_line_ends = clip);
        }
    }

    pub fn capability(&self, cx: &App) -> Capability {
        if self.read_only {
            Capability::ReadOnly
        } else {
            self.buffer.read(cx).capability()
        }
    }

    pub fn read_only(&self, cx: &App) -> bool {
        self.read_only || self.buffer.read(cx).read_only()
    }

    pub fn set_read_only(&mut self, read_only: bool) {
        self.read_only = read_only;
    }

    pub fn set_use_selection_highlight(&mut self, highlight: bool) {
        self.use_selection_highlight = highlight;
    }

    pub fn set_should_serialize(&mut self, should_serialize: bool, cx: &App) {
        self.buffer_serialization = should_serialize.then(|| {
            BufferSerialization::new(
                ProjectSettings::get_global(cx)
                    .session
                    .restore_unsaved_buffers,
            )
        })
    }

    pub(crate) fn should_serialize_buffer(&self) -> bool {
        self.buffer_serialization.is_some()
    }

    pub fn set_use_modal_editing(&mut self, to: bool) {
        self.use_modal_editing = to;
    }

    pub fn use_modal_editing(&self) -> bool {
        self.use_modal_editing
    }

    pub fn has_mouse_context_menu(&self) -> bool {
        self.mouse_context_menu.is_some()
    }
}
