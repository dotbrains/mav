use super::*;

impl Vim {
    pub(super) fn sync_vim_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let state = self.state_for_editor_settings(cx);
        self.update_editor(cx, |_, editor, cx| {
            Vim::sync_vim_settings_to_editor(&state, editor, window, cx);
        });
        cx.notify()
    }

    pub(super) fn state_for_editor_settings(&self, cx: &App) -> VimEditorSettingsState {
        VimEditorSettingsState {
            cursor_shape: self.cursor_shape(cx),
            clip_at_line_ends: self.clip_at_line_ends(),
            collapse_matches: !HelixModeSetting::get_global(cx).0 && !self.search.cmd_f_search,
            input_enabled: self.editor_input_enabled(),
            expects_character_input: self.expects_character_input(),
            autoindent: self.should_autoindent(),
            cursor_offset_on_selection: self.mode.has_selection(),
            line_mode: matches!(self.mode, Mode::VisualLine),
            hide_edit_predictions: !matches!(self.mode, Mode::Insert | Mode::Replace)
                && !(self.mode.is_normal()
                    && VimSettings::get_global(cx).show_edit_predictions_in_normal_mode),
        }
    }

    pub(super) fn sync_vim_settings_to_editor(
        state: &VimEditorSettingsState,
        editor: &mut Editor,
        window: &mut Window,
        cx: &mut Context<Editor>,
    ) {
        editor.set_cursor_shape(state.cursor_shape, cx);
        editor.set_clip_at_line_ends(state.clip_at_line_ends, cx);
        editor.set_collapse_matches(state.collapse_matches);
        editor.set_input_enabled(state.input_enabled);
        editor.set_expects_character_input(state.expects_character_input);
        editor.set_autoindent(state.autoindent);
        editor.set_cursor_offset_on_selection(state.cursor_offset_on_selection);
        editor.selections.set_line_mode(state.line_mode);
        editor.set_edit_predictions_hidden_for_vim_mode(state.hide_edit_predictions, window, cx);
    }

    pub(super) fn set_status_label(
        &mut self,
        label: impl Into<SharedString>,
        cx: &mut Context<Editor>,
    ) {
        self.status_label = Some(label.into());
        cx.notify();
    }
}
struct VimEditorSettingsState {
    cursor_shape: CursorShape,
    clip_at_line_ends: bool,
    collapse_matches: bool,
    input_enabled: bool,
    expects_character_input: bool,
    autoindent: bool,
    cursor_offset_on_selection: bool,
    line_mode: bool,
    hide_edit_predictions: bool,
}

#[derive(Clone, RegisterSetting)]
struct VimSettings {
    pub default_mode: Mode,
    pub toggle_relative_line_numbers: bool,
    pub use_system_clipboard: settings::UseSystemClipboard,
    pub use_smartcase_find: bool,
    pub use_regex_search: bool,
    pub gdefault: bool,
    pub custom_digraphs: HashMap<String, Arc<str>>,
    pub highlight_on_yank_duration: u64,
    pub cursor_shape: CursorShapeSettings,
    pub show_edit_predictions_in_normal_mode: bool,
}

/// Cursor shape configuration for insert mode.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum InsertModeCursorShape {
    /// Inherit cursor shape from the editor's base cursor_shape setting.
    /// This allows users to set their preferred editor cursor and have
    /// it automatically apply to vim insert mode.
    Inherit,
    /// Use an explicit cursor shape for insert mode.
    Explicit(CursorShape),
}

/// The settings for cursor shape.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct CursorShapeSettings {
    /// Cursor shape for the normal mode.
    ///
    /// Default: block
    pub normal: CursorShape,
    /// Cursor shape for the replace mode.
    ///
    /// Default: underline
    pub replace: CursorShape,
    /// Cursor shape for the visual mode.
    ///
    /// Default: block
    pub visual: CursorShape,
    /// Cursor shape for the insert mode.
    ///
    /// Default: Inherit (follows editor.cursor_shape)
    pub insert: InsertModeCursorShape,
}

impl From<settings::VimInsertModeCursorShape> for InsertModeCursorShape {
    fn from(shape: settings::VimInsertModeCursorShape) -> Self {
        match shape {
            settings::VimInsertModeCursorShape::Inherit => InsertModeCursorShape::Inherit,
            settings::VimInsertModeCursorShape::Bar => {
                InsertModeCursorShape::Explicit(CursorShape::Bar)
            }
            settings::VimInsertModeCursorShape::Block => {
                InsertModeCursorShape::Explicit(CursorShape::Block)
            }
            settings::VimInsertModeCursorShape::Underline => {
                InsertModeCursorShape::Explicit(CursorShape::Underline)
            }
            settings::VimInsertModeCursorShape::Hollow => {
                InsertModeCursorShape::Explicit(CursorShape::Hollow)
            }
        }
    }
}

impl From<settings::CursorShapeSettings> for CursorShapeSettings {
    fn from(settings: settings::CursorShapeSettings) -> Self {
        Self {
            normal: settings.normal.unwrap().into(),
            replace: settings.replace.unwrap().into(),
            visual: settings.visual.unwrap().into(),
            insert: settings.insert.unwrap().into(),
        }
    }
}

impl From<settings::ModeContent> for Mode {
    fn from(mode: ModeContent) -> Self {
        match mode {
            ModeContent::Normal => Self::Normal,
            ModeContent::Insert => Self::Insert,
        }
    }
}

impl Settings for VimSettings {
    fn from_settings(content: &settings::SettingsContent) -> Self {
        let vim = content.vim.clone().unwrap();
        Self {
            default_mode: vim.default_mode.unwrap().into(),
            toggle_relative_line_numbers: vim.toggle_relative_line_numbers.unwrap(),
            use_system_clipboard: vim.use_system_clipboard.unwrap(),
            use_smartcase_find: vim.use_smartcase_find.unwrap(),
            use_regex_search: vim.use_regex_search.unwrap(),
            gdefault: vim.gdefault.unwrap(),
            custom_digraphs: vim.custom_digraphs.unwrap(),
            highlight_on_yank_duration: vim.highlight_on_yank_duration.unwrap(),
            cursor_shape: vim.cursor_shape.unwrap().into(),
            show_edit_predictions_in_normal_mode: vim.show_edit_predictions_in_normal_mode.unwrap(),
        }
    }
}
