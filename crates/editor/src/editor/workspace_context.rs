use super::*;

impl Editor {
    pub fn key_context(&self, window: &mut Window, cx: &mut App) -> KeyContext {
        self.key_context_internal(self.has_active_edit_prediction(), window, cx)
    }

    pub(crate) fn key_context_internal(
        &self,
        has_active_edit_prediction: bool,
        window: &mut Window,
        cx: &mut App,
    ) -> KeyContext {
        let mut key_context = KeyContext::new_with_defaults();
        key_context.add("Editor");
        let mode = match self.mode {
            EditorMode::SingleLine => "single_line",
            EditorMode::AutoHeight { .. } => "auto_height",
            EditorMode::Minimap { .. } => "minimap",
            EditorMode::Full { .. } => "full",
        };

        if EditorSettings::jupyter_enabled(cx) {
            key_context.add("jupyter");
        }

        key_context.set("mode", mode);
        if self.pending_rename.is_some() {
            key_context.add("renaming");
        }

        if let Some(snippet_stack) = self.snippet_stack.last() {
            key_context.add("in_snippet");

            if snippet_stack.active_index > 0 {
                key_context.add("has_previous_tabstop");
            }

            if snippet_stack.active_index < snippet_stack.ranges.len().saturating_sub(1) {
                key_context.add("has_next_tabstop");
            }
        }

        match self.context_menu.borrow().as_ref() {
            Some(CodeContextMenu::Completions(menu)) => {
                if menu.visible() {
                    key_context.add("menu");
                    key_context.add("showing_completions");
                }
            }
            Some(CodeContextMenu::CodeActions(menu)) => {
                if menu.visible() {
                    key_context.add("menu");
                    key_context.add("showing_code_actions")
                }
            }
            None => {}
        }

        if self.signature_help_state.has_multiple_signatures() {
            key_context.add("showing_signature_help");
        }

        // Disable vim contexts when a sub-editor (e.g. rename/inline assistant) is focused.
        if !self.focus_handle(cx).contains_focused(window, cx)
            || (self.is_focused(window) || self.mouse_menu_is_focused(window, cx))
        {
            for addon in self.addons.values() {
                addon.extend_key_context(&mut key_context, cx)
            }
        }

        if let Some(singleton_buffer) = self.buffer.read(cx).as_singleton() {
            if let Some(extension) = singleton_buffer.read(cx).file().and_then(|file| {
                Some(
                    file.full_path(cx)
                        .extension()?
                        .to_string_lossy()
                        .to_lowercase(),
                )
            }) {
                key_context.set("extension", extension);
            }
        } else {
            key_context.add("multibuffer");
        }

        if has_active_edit_prediction {
            key_context.add(EDIT_PREDICTION_KEY_CONTEXT);
            key_context.add("copilot_suggestion");
        }

        if self.in_leading_whitespace {
            key_context.add("in_leading_whitespace");
        }
        if self.edit_prediction_requires_modifier() {
            key_context.set("edit_prediction_mode", "subtle")
        } else {
            key_context.set("edit_prediction_mode", "eager");
        }

        if self.selection_mark_mode {
            key_context.add("selection_mode");
        }

        let disjoint = self.selections.disjoint_anchors();
        if matches!(
            &self.mode,
            EditorMode::SingleLine | EditorMode::AutoHeight { .. }
        ) && let [selection] = disjoint
            && selection.start == selection.end
        {
            let snapshot = self.snapshot(window, cx);
            let snapshot = snapshot.buffer_snapshot();
            let caret_offset = selection.end.to_offset(snapshot);

            if caret_offset == MultiBufferOffset(0) {
                key_context.add("start_of_input");
            }

            if caret_offset == snapshot.len() {
                key_context.add("end_of_input");
            }
        }

        if self.has_any_expanded_diff_hunks(cx) {
            key_context.add("diffs_expanded");
        }

        key_context
    }

    pub fn working_directory(&self, cx: &App) -> Option<PathBuf> {
        if let Some(buffer) = self.buffer().read(cx).as_singleton() {
            if let Some(file) = buffer.read(cx).file().and_then(|f| f.as_local())
                && let Some(dir) = file.abs_path(cx).parent()
            {
                return Some(dir.to_owned());
            }
        }

        None
    }

    pub fn target_file_abs_path(&self, cx: &mut Context<Self>) -> Option<PathBuf> {
        self.active_buffer(cx).and_then(|buffer| {
            let buffer = buffer.read(cx);
            if let Some(project_path) = buffer.project_path(cx) {
                let project = self.project()?.read(cx);
                project.absolute_path(&project_path, cx)
            } else {
                buffer
                    .file()
                    .and_then(|file| file.as_local().map(|file| file.abs_path(cx)))
            }
        })
    }

    pub fn selection_menu_enabled(&self, cx: &App) -> bool {
        self.show_selection_menu
            .unwrap_or_else(|| EditorSettings::get_global(cx).toolbar.selections_menu)
    }

    pub fn toggle_selection_menu(
        &mut self,
        _: &ToggleSelectionMenu,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show_selection_menu = self
            .show_selection_menu
            .map(|show_selections_menu| !show_selections_menu)
            .or_else(|| Some(!EditorSettings::get_global(cx).toolbar.selections_menu));

        cx.notify();
    }

    pub fn new_file(
        workspace: &mut Workspace,
        _: &workspace::NewFile,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) {
        Self::new_in_workspace(workspace, window, cx).detach_and_prompt_err(
            "Failed to create buffer",
            window,
            cx,
            |e, _, _| match e.error_code() {
                ErrorCode::RemoteUpgradeRequired => Some(format!(
                    "The remote instance of Mav does not support this yet. It must be upgraded to {}",
                    e.error_tag("required").unwrap_or("the latest version")
                )),
                _ => None,
            },
        );
    }

    pub fn new_in_workspace(
        workspace: &mut Workspace,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) -> Task<Result<Entity<Editor>>> {
        let project = workspace.project().clone();
        let create = project.update(cx, |project, cx| project.create_buffer(None, true, cx));

        cx.spawn_in(window, async move |workspace, cx| {
            let buffer = create.await?;
            workspace.update_in(cx, |workspace, window, cx| {
                let editor =
                    cx.new(|cx| Editor::for_buffer(buffer, Some(project.clone()), window, cx));
                workspace.add_item_to_active_pane(Box::new(editor.clone()), None, true, window, cx);
                editor
            })
        })
    }

    pub(crate) fn new_file_vertical(
        workspace: &mut Workspace,
        _: &workspace::NewFileSplitVertical,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) {
        Self::new_file_in_direction(workspace, SplitDirection::vertical(cx), window, cx)
    }

    pub(crate) fn new_file_horizontal(
        workspace: &mut Workspace,
        _: &workspace::NewFileSplitHorizontal,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) {
        Self::new_file_in_direction(workspace, SplitDirection::horizontal(cx), window, cx)
    }

    pub(crate) fn new_file_split(
        workspace: &mut Workspace,
        action: &workspace::NewFileSplit,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) {
        Self::new_file_in_direction(workspace, action.0, window, cx)
    }

    pub(crate) fn new_file_in_direction(
        workspace: &mut Workspace,
        direction: SplitDirection,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) {
        let project = workspace.project().clone();
        let create = project.update(cx, |project, cx| project.create_buffer(None, true, cx));

        cx.spawn_in(window, async move |workspace, cx| {
            let buffer = create.await?;
            workspace.update_in(cx, move |workspace, window, cx| {
                workspace.split_item(
                    direction,
                    Box::new(
                        cx.new(|cx| Editor::for_buffer(buffer, Some(project.clone()), window, cx)),
                    ),
                    window,
                    cx,
                )
            })?;
            anyhow::Ok(())
        })
        .detach_and_prompt_err("Failed to create buffer", window, cx, |e, _, _| {
            match e.error_code() {
                ErrorCode::RemoteUpgradeRequired => Some(format!(
                    "The remote instance of Mav does not support this yet. It must be upgraded to {}",
                    e.error_tag("required").unwrap_or("the latest version")
                )),
                _ => None,
            }
        });
    }
}
