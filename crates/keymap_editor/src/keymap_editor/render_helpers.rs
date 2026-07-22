use super::*;

pub(super) fn row_group_id(row_index: usize) -> SharedString {
    SharedString::new(format!("keymap-table-row-{}", row_index))
}

pub(super) fn base_button_style(row_index: usize, icon: IconName) -> IconButton {
    IconButton::new(("keymap-icon", row_index), icon)
        .shape(IconButtonShape::Square)
        .size(ButtonSize::Compact)
}

#[derive(Debug, Clone, IntoElement)]
pub(super) struct SyntaxHighlightedText {
    text: SharedString,
    language: Arc<Language>,
}

impl SyntaxHighlightedText {
    pub fn new(text: impl Into<SharedString>, language: Arc<Language>) -> Self {
        Self {
            text: text.into(),
            language,
        }
    }
}

impl RenderOnce for SyntaxHighlightedText {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let text_style = window.text_style();
        let syntax_theme = cx.theme().syntax();

        let text = self.text.clone();

        let highlights = self
            .language
            .highlight_text(&text.as_ref().into(), 0..text.len());
        let mut runs = Vec::with_capacity(highlights.len());
        let mut offset = 0;

        for (highlight_range, highlight_id) in highlights {
            // Add un-highlighted text before the current highlight
            if highlight_range.start > offset {
                runs.push(text_style.to_run(highlight_range.start - offset));
            }

            let mut run_style = text_style.clone();
            if let Some(highlight_style) = syntax_theme.get(highlight_id).cloned() {
                run_style = run_style.highlight(highlight_style);
            }

            // add the highlighted range
            runs.push(run_style.to_run(highlight_range.len()));
            offset = highlight_range.end;
        }

        // Add any remaining un-highlighted text
        if offset < text.len() {
            runs.push(text_style.to_run(text.len() - offset));
        }

        StyledText::new(text).with_runs(runs)
    }
}

#[derive(PartialEq)]
pub(super) struct InputError {
    severity: Severity,
    content: SharedString,
}

impl InputError {
    fn warning(message: impl Into<SharedString>) -> Self {
        Self {
            severity: Severity::Warning,
            content: message.into(),
        }
    }

    fn error(message: anyhow::Error) -> Self {
        Self {
            severity: Severity::Error,
            content: message.to_string().into(),
        }
    }
}

pub(super) struct KeybindingEditorModal {
    creating: bool,
    editing_keybind: ProcessedBinding,
    editing_keybind_idx: usize,
    keybind_editor: Entity<KeystrokeInput>,
    context_editor: Entity<InputField>,
    action_editor: Option<Entity<InputField>>,
    action_arguments_editor: Option<Entity<ActionArgumentsEditor>>,
    action_name_to_static: HashMap<String, &'static str>,
    selected_action_name: Option<&'static str>,
    fs: Arc<dyn Fs>,
    error: Option<InputError>,
    keymap_editor: Entity<KeymapEditor>,
    workspace: WeakEntity<Workspace>,
    focus_state: KeybindingEditorModalFocusState,
}

impl ModalView for KeybindingEditorModal {}

impl EventEmitter<DismissEvent> for KeybindingEditorModal {}

impl Focusable for KeybindingEditorModal {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        if let Some(action_editor) = &self.action_editor {
            return action_editor.focus_handle(cx);
        }
        self.keybind_editor.focus_handle(cx)
    }
}
