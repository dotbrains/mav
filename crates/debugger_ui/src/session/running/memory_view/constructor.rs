use super::*;

impl MemoryView {
    pub(crate) fn new(
        session: Entity<Session>,
        workspace: WeakEntity<Workspace>,
        stack_frame_list: WeakEntity<StackFrameList>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let view_state_handle = ViewStateHandle::new(0, WIDTHS[4].clone());

        let query_editor = cx.new(|cx| Editor::single_line(window, cx));

        let mut this = Self {
            workspace,
            stack_frame_list,
            focus_handle: cx.focus_handle(),
            view_state_handle,
            query_editor,
            session,
            width_picker_handle: Default::default(),
            is_writing_memory: true,
            open_context_menu: None,
        };
        this.change_query_bar_mode(false, window, cx);
        cx.on_focus_out(&this.focus_handle, window, |this, _, window, cx| {
            this.change_query_bar_mode(false, window, cx);
            cx.notify();
        })
        .detach();
        this
    }

    pub(super) fn view_state(&self) -> RefMut<'_, ViewState> {
        self.view_state_handle.0.borrow_mut()
    }

    pub(super) fn render_query_bar(&self, cx: &Context<Self>) -> impl IntoElement {
        EditorElement::new(
            &self.query_editor,
            Self::editor_style(&self.query_editor, cx),
        )
    }

    pub(super) fn editor_style(editor: &Entity<Editor>, cx: &Context<Self>) -> EditorStyle {
        let is_read_only = editor.read(cx).read_only(cx);
        let settings = ThemeSettings::get_global(cx);
        let theme = cx.theme();
        let text_style = TextStyle {
            color: if is_read_only {
                theme.colors().text_muted
            } else {
                theme.colors().text
            },
            font_family: settings.buffer_font.family.clone(),
            font_features: settings.buffer_font.features.clone(),
            font_size: TextSize::Small.rems(cx).into(),
            font_weight: settings.buffer_font.weight,

            ..Default::default()
        };
        EditorStyle {
            background: theme.colors().editor_background,
            local_player: theme.players().local(),
            text: text_style,
            ..Default::default()
        }
    }

    pub(super) fn render_width_picker(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> DropdownMenu {
        let weak = cx.weak_entity();
        let selected_width = self.view_state().line_width.clone();
        DropdownMenu::new(
            "memory-view-width-picker",
            selected_width.label.clone(),
            ContextMenu::build(window, cx, |mut this, window, cx| {
                for width in &WIDTHS {
                    let weak = weak.clone();
                    let width = width.clone();
                    this = this.entry(width.label.clone(), None, move |_, cx| {
                        _ = weak.update(cx, |this, _| {
                            let mut view_state = this.view_state();
                            // Convert base ix between 2 line widths to keep the shown memory address roughly the same.
                            // All widths are powers of 2, so the conversion should be lossless.
                            match view_state.line_width.width.cmp(&width.width) {
                                std::cmp::Ordering::Less => {
                                    // We're converting up.
                                    let shift = width.width.trailing_zeros()
                                        - view_state.line_width.width.trailing_zeros();
                                    view_state.base_row >>= shift;
                                }
                                std::cmp::Ordering::Greater => {
                                    // We're converting down.
                                    let shift = view_state.line_width.width.trailing_zeros()
                                        - width.width.trailing_zeros();
                                    view_state.base_row <<= shift;
                                }
                                _ => {}
                            }
                            view_state.line_width = width.clone();
                        });
                    });
                }
                if let Some(ix) = WIDTHS
                    .iter()
                    .position(|width| width.width == selected_width.width)
                {
                    for _ in 0..=ix {
                        this.select_next(&Default::default(), window, cx);
                    }
                }
                this
            }),
        )
        .handle(self.width_picker_handle.clone())
    }
}
