use super::*;

#[derive(Clone)]
pub(crate) struct ErasedEditorImpl(pub(crate) Entity<Editor>);

impl ui_input::ErasedEditor for ErasedEditorImpl {
    fn text(&self, cx: &App) -> String {
        self.0.read(cx).text(cx)
    }

    fn set_text(&self, text: &str, window: &mut Window, cx: &mut App) {
        self.0.update(cx, |this, cx| {
            this.set_text(text, window, cx);
        })
    }

    fn clear(&self, window: &mut Window, cx: &mut App) {
        self.0.update(cx, |this, cx| this.clear(window, cx));
    }

    fn set_placeholder_text(&self, text: &str, window: &mut Window, cx: &mut App) {
        self.0.update(cx, |this, cx| {
            this.set_placeholder_text(text, window, cx);
        });
    }

    fn set_multiline(&self, max_lines: Option<usize>, _window: &mut Window, cx: &mut App) {
        self.0.update(cx, |this, cx| {
            if let Some(max_lines) = max_lines {
                this.set_mode(EditorMode::AutoHeight {
                    min_lines: 1,
                    max_lines: Some(max_lines),
                });
                this.set_soft_wrap_mode(language_settings::SoftWrap::EditorWidth, cx);
            } else {
                this.set_mode(EditorMode::SingleLine);
            }
            cx.notify();
        });
    }

    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.0.read(cx).focus_handle(cx)
    }

    fn render(&self, _: &mut Window, cx: &App) -> AnyElement {
        let settings = ThemeSettings::get_global(cx);
        let theme_color = cx.theme().colors();

        let text_style = TextStyle {
            font_family: settings.ui_font.family.clone(),
            font_features: settings.ui_font.features.clone(),
            font_size: rems(0.875).into(),
            font_weight: settings.ui_font.weight,
            font_style: FontStyle::Normal,
            line_height: relative(1.2),
            color: theme_color.text,
            ..Default::default()
        };
        let editor_style = EditorStyle {
            background: theme_color.ghost_element_background,
            local_player: cx.theme().players().local(),
            syntax: cx.theme().syntax().clone(),
            text: text_style,
            ..Default::default()
        };
        EditorElement::new(&self.0, editor_style).into_any()
    }

    fn as_any(&self) -> &dyn Any {
        &self.0
    }

    fn move_selection_to_end(&self, window: &mut Window, cx: &mut App) {
        self.0.update(cx, |editor, cx| {
            let editor_offset = editor.buffer().read(cx).len(cx);
            editor.change_selections(
                SelectionEffects::scroll(Autoscroll::Next),
                window,
                cx,
                |s| s.select_ranges(Some(editor_offset..editor_offset)),
            );
        });
    }

    fn select_all(&self, window: &mut Window, cx: &mut App) {
        self.0.update(cx, |editor, cx| {
            editor.select_all(&Default::default(), window, cx);
        });
    }

    fn subscribe(
        &self,
        mut callback: Box<dyn FnMut(ui_input::ErasedEditorEvent, &mut Window, &mut App) + 'static>,
        window: &mut Window,
        cx: &mut App,
    ) -> Subscription {
        window.subscribe(&self.0, cx, move |_, event: &EditorEvent, window, cx| {
            let event = match event {
                EditorEvent::BufferEdited => ui_input::ErasedEditorEvent::BufferEdited,
                EditorEvent::Blurred => ui_input::ErasedEditorEvent::Blurred,
                _ => return,
            };
            (callback)(event, window, cx);
        })
    }

    fn set_masked(&self, masked: bool, _window: &mut Window, cx: &mut App) {
        self.0.update(cx, |editor, cx| {
            editor.set_masked(masked, cx);
        });
    }

    fn set_read_only(&self, read_only: bool, cx: &mut App) {
        self.0.update(cx, |editor, cx| {
            editor.set_read_only(read_only);
            cx.notify();
        });
    }
}
