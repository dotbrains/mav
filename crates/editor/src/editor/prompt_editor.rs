use super::*;

#[derive(Copy, Clone, Debug)]
pub(super) enum BreakpointPromptEditAction {
    Log,
    Condition,
    HitCondition,
}

pub(super) type PromptEditorCallback =
    Box<dyn FnOnce(String, &mut Editor, &mut Context<Editor>) + 'static>;

pub(super) struct PromptEditor {
    pub(crate) prompt: Entity<Editor>,
    editor: WeakEntity<Editor>,
    confirm_callback: Option<PromptEditorCallback>,
    cancel_callback: Option<PromptEditorCallback>,
    block_ids: HashSet<CustomBlockId>,
    pub(crate) editor_margins: Arc<Mutex<EditorMargins>>,
    _subscriptions: Vec<Subscription>,
}

impl PromptEditor {
    const MAX_LINES: u8 = 4;

    pub(super) fn new(
        editor: WeakEntity<Editor>,
        placeholder_text: &str,
        base_text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let buffer = cx.new(|cx| Buffer::local(base_text, cx));
        let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));

        let prompt = cx.new(|cx| {
            let mut prompt = Editor::new(
                EditorMode::AutoHeight {
                    min_lines: 1,
                    max_lines: Some(Self::MAX_LINES as usize),
                },
                buffer,
                None,
                window,
                cx,
            );
            prompt.set_soft_wrap_mode(language::language_settings::SoftWrap::EditorWidth, cx);
            prompt.set_show_cursor_when_unfocused(false, cx);
            prompt.set_placeholder_text(placeholder_text, window, cx);

            prompt
        });

        Self {
            prompt,
            editor,
            confirm_callback: None,
            cancel_callback: None,
            editor_margins: Arc::new(Mutex::new(EditorMargins::default())),
            block_ids: Default::default(),
            _subscriptions: vec![],
        }
    }

    pub(super) fn on_confirm(mut self, confirm: PromptEditorCallback) -> Self {
        self.confirm_callback = Some(confirm);
        self
    }

    pub(super) fn on_cancel(mut self, cancel: PromptEditorCallback) -> Self {
        self.cancel_callback = Some(cancel);
        self
    }

    pub(crate) fn add_block_ids(&mut self, block_ids: Vec<CustomBlockId>) {
        self.block_ids.extend(block_ids)
    }

    fn confirm(&mut self, _: &menu::Confirm, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(editor) = self.editor.upgrade() {
            let message = self.message(cx);

            editor.update(cx, |editor, cx| {
                if let Some(confirm) = self.confirm_callback.take() {
                    confirm(message, editor, cx);
                }

                editor.remove_blocks(self.block_ids.clone(), None, cx);
                cx.focus_self(window);
            });
        }
    }

    fn cancel(&mut self, _: &menu::Cancel, window: &mut Window, cx: &mut Context<Self>) {
        let message = self.message(cx);
        self.editor
            .update(cx, |editor, cx| {
                if let Some(cancel) = self.cancel_callback.take() {
                    cancel(message, editor, cx);
                }

                editor.remove_blocks(self.block_ids.clone(), None, cx);
                window.focus(&editor.focus_handle, cx);
            })
            .log_err();
    }

    fn message(&self, cx: &App) -> String {
        self.prompt
            .read(cx)
            .buffer
            .read(cx)
            .as_singleton()
            .expect("A multi buffer in prompt isn't possible")
            .read(cx)
            .as_rope()
            .to_string()
    }

    fn render_prompt_editor(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let settings = ThemeSettings::get_global(cx);
        let text_style = TextStyle {
            color: if self.prompt.read(cx).read_only(cx) {
                cx.theme().colors().text_disabled
            } else {
                cx.theme().colors().text
            },
            font_family: settings.buffer_font.family.clone(),
            font_fallbacks: settings.buffer_font.fallbacks.clone(),
            font_size: settings.buffer_font_size(cx).into(),
            font_weight: settings.buffer_font.weight,
            line_height: relative(settings.buffer_line_height.value()),
            ..Default::default()
        };
        EditorElement::new(
            &self.prompt,
            EditorStyle {
                background: cx.theme().colors().editor_background,
                local_player: cx.theme().players().local(),
                text: text_style,
                ..Default::default()
            },
        )
    }

    fn render_close_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let focus_handle = self.prompt.focus_handle(cx);
        IconButton::new("cancel", IconName::Close)
            .icon_color(Color::Muted)
            .shape(IconButtonShape::Square)
            .tooltip(move |_window, cx| {
                Tooltip::for_action_in("Cancel", &menu::Cancel, &focus_handle, cx)
            })
            .on_click(cx.listener(|this, _, window, cx| {
                this.cancel(&menu::Cancel, window, cx);
            }))
    }

    fn render_confirm_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let focus_handle = self.prompt.focus_handle(cx);
        IconButton::new("confirm", IconName::Return)
            .icon_color(Color::Muted)
            .shape(IconButtonShape::Square)
            .tooltip(move |_window, cx| {
                Tooltip::for_action_in("Confirm", &menu::Confirm, &focus_handle, cx)
            })
            .on_click(cx.listener(|this, _, window, cx| {
                this.confirm(&menu::Confirm, window, cx);
            }))
    }
}

impl Render for PromptEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ui_font_size = ThemeSettings::get_global(cx).ui_font_size(cx);
        let editor_margins = *self.editor_margins.lock();
        let gutter_dimensions = editor_margins.gutter;
        let left_gutter_width = gutter_dimensions.full_width() + (gutter_dimensions.margin / 2.0);
        let right_padding = editor_margins.right + px(9.);
        h_flex()
            .key_context("Editor")
            .bg(cx.theme().colors().editor_background)
            .border_y_1()
            .border_color(cx.theme().status().info_border)
            .size_full()
            .py(window.line_height() / 2.5)
            .pr(right_padding)
            .on_action(cx.listener(Self::confirm))
            .on_action(cx.listener(Self::cancel))
            .child(
                WithRemSize::new(ui_font_size)
                    .h_full()
                    .w(left_gutter_width)
                    .flex()
                    .flex_row()
                    .flex_shrink_0()
                    .items_center()
                    .justify_center()
                    .gap_1()
                    .child(self.render_close_button(cx)),
            )
            .child(
                h_flex()
                    .w_full()
                    .justify_between()
                    .child(div().flex_1().child(self.render_prompt_editor(cx)))
                    .child(
                        WithRemSize::new(ui_font_size)
                            .flex()
                            .flex_row()
                            .items_center()
                            .child(self.render_confirm_button(cx)),
                    ),
            )
    }
}

impl Focusable for PromptEditor {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.prompt.focus_handle(cx)
    }
}
