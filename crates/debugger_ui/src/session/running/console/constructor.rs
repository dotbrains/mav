use super::*;

impl Console {
    pub fn new(
        session: Entity<Session>,
        stack_frame_list: Entity<StackFrameList>,
        variable_list: Entity<VariableList>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let console = cx.new(|cx| {
            let mut editor = Editor::multi_line(window, cx);
            editor.set_mode(EditorMode::Full {
                scale_ui_elements_with_buffer_font_size: true,
                show_active_line_background: true,
                sizing_behavior: SizingBehavior::ExcludeOverscrollMargin,
            });
            editor.move_to_end(&editor::actions::MoveToEnd, window, cx);
            editor.set_read_only(true);
            editor.disable_scrollbars_and_minimap(window, cx);
            editor.set_show_gutter(false, cx);
            editor.set_show_runnables(false, cx);
            editor.set_show_bookmarks(false, cx);
            editor.set_show_breakpoints(false, cx);
            editor.set_show_code_actions(false, cx);
            editor.set_show_line_numbers(false, cx);
            editor.set_show_git_diff_gutter(false, cx);
            editor.set_autoindent(false);
            editor.set_input_enabled(false);
            editor.set_use_autoclose(false);
            editor.set_show_wrap_guides(false, cx);
            editor.set_show_indent_guides(false, cx);
            editor.set_show_edit_predictions(Some(false), window, cx);
            editor.set_use_modal_editing(false);
            editor.disable_mouse_wheel_zoom();
            editor.set_soft_wrap_mode(language::language_settings::SoftWrap::EditorWidth, cx);
            editor
        });
        let focus_handle = cx.focus_handle();

        let this = cx.weak_entity();
        let query_bar = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_placeholder_text("Evaluate an expression", window, cx);
            editor.set_use_autoclose(false);
            editor.set_show_gutter(false, cx);
            editor.set_show_wrap_guides(false, cx);
            editor.set_show_indent_guides(false, cx);
            editor.set_completion_provider(Some(Rc::new(ConsoleQueryBarCompletionProvider(this))));

            editor
        });

        let _subscriptions = vec![
            cx.subscribe(&stack_frame_list, Self::handle_stack_frame_list_events),
            cx.on_focus(&focus_handle, window, |console, window, cx| {
                if console.is_running(cx) {
                    console.query_bar.focus_handle(cx).focus(window, cx);
                }
            }),
        ];

        Self {
            session,
            console,
            query_bar,
            variable_list,
            _subscriptions,
            stack_frame_list,
            update_output_task: None,
            last_token: OutputToken(0),
            focus_handle,
            history: SearchHistory::new(
                None,
                project::search_history::QueryInsertionBehavior::ReplacePreviousIfContains,
            ),
            cursor: Default::default(),
        }
    }

    #[cfg(test)]
    pub(crate) fn editor(&self) -> &Entity<Editor> {
        &self.console
    }

    pub(super) fn is_running(&self, cx: &Context<Self>) -> bool {
        self.session.read(cx).is_started()
    }

    pub(super) fn handle_stack_frame_list_events(
        &mut self,
        _: Entity<StackFrameList>,
        event: &StackFrameListEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            StackFrameListEvent::SelectedStackFrameChanged(_) => cx.notify(),
            StackFrameListEvent::BuiltEntries => {}
        }
    }

    pub(crate) fn show_indicator(&self, cx: &App) -> bool {
        self.session.read(cx).has_new_output(self.last_token)
    }
}
