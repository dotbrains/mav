use gpui::{App, HighlightStyle, Hsla, Pixels, TextStyle, WeakEntity};
use language::language_settings::AllLanguageSettings;
use settings::Settings;
use std::sync::Arc;
use theme::{ActiveTheme as _, PlayerColor, StatusColors, SyntaxTheme};

use crate::{Editor, EditorSettings, display_map::EditPredictionStyles};

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Navigated {
    Yes,
    No,
}

impl Navigated {
    pub fn from_bool(yes: bool) -> Navigated {
        if yes { Navigated::Yes } else { Navigated::No }
    }
}

#[derive(Copy, Clone, Default, PartialEq, Eq, Debug)]
pub enum SizingBehavior {
    /// The editor will layout itself using `size_full` and will include the vertical
    /// scroll margin as requested by user settings.
    #[default]
    Default,
    /// The editor will layout itself using `size_full`, but will not have any
    /// vertical overscroll.
    ExcludeOverscrollMargin,
    /// The editor will request a vertical size according to its content and will be
    /// layouted without a vertical scroll margin.
    SizeByContent,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum EditorMode {
    SingleLine,
    AutoHeight {
        min_lines: usize,
        max_lines: Option<usize>,
    },
    Full {
        /// When set to `true`, the editor will scale its UI elements with the buffer font size.
        scale_ui_elements_with_buffer_font_size: bool,
        /// When set to `true`, the editor will render a background for the active line.
        show_active_line_background: bool,
        /// Determines the sizing behavior for this editor.
        sizing_behavior: SizingBehavior,
    },
    Minimap {
        parent: WeakEntity<Editor>,
    },
}

impl EditorMode {
    pub fn full() -> Self {
        Self::Full {
            scale_ui_elements_with_buffer_font_size: true,
            show_active_line_background: true,
            sizing_behavior: SizingBehavior::Default,
        }
    }

    #[inline]
    pub fn is_full(&self) -> bool {
        matches!(self, Self::Full { .. })
    }

    #[inline]
    pub fn is_single_line(&self) -> bool {
        matches!(self, Self::SingleLine { .. })
    }

    #[inline]
    pub(crate) fn is_minimap(&self) -> bool {
        matches!(self, Self::Minimap { .. })
    }
}

#[derive(Copy, Clone, Debug)]
pub enum SoftWrap {
    /// Prefer not to wrap at all.
    ///
    /// Note: this is currently internal, as actually limited by [`crate::MAX_LINE_LEN`] until it wraps.
    /// The mode is used inside git diff hunks, where it's seems currently more useful to not wrap as much as possible.
    GitDiff,
    /// Prefer a single line generally, unless an overly long line is encountered.
    None,
    /// Soft wrap lines that exceed the editor width.
    EditorWidth,
    /// Soft wrap line at the preferred line length or the editor width (whichever is smaller).
    Bounded(u32),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MinimapVisibility {
    Disabled,
    Enabled {
        /// The configuration currently present in the users settings.
        setting_configuration: bool,
        /// Whether to override the currently set visibility from the users setting.
        toggle_override: bool,
    },
}

impl MinimapVisibility {
    pub(crate) fn for_mode(mode: &EditorMode, cx: &App) -> Self {
        if mode.is_full() {
            Self::Enabled {
                setting_configuration: EditorSettings::get_global(cx).minimap.minimap_enabled(),
                toggle_override: false,
            }
        } else {
            Self::Disabled
        }
    }

    pub(crate) fn hidden(&self) -> Self {
        match *self {
            Self::Enabled {
                setting_configuration,
                ..
            } => Self::Enabled {
                setting_configuration,
                toggle_override: setting_configuration,
            },
            Self::Disabled => Self::Disabled,
        }
    }

    pub(crate) fn disabled(&self) -> bool {
        matches!(*self, Self::Disabled)
    }

    pub(crate) fn settings_visibility(&self) -> bool {
        match *self {
            Self::Enabled {
                setting_configuration,
                ..
            } => setting_configuration,
            _ => false,
        }
    }

    pub(crate) fn visible(&self) -> bool {
        match *self {
            Self::Enabled {
                setting_configuration,
                toggle_override,
            } => setting_configuration ^ toggle_override,
            _ => false,
        }
    }

    pub(crate) fn toggle_visibility(&self) -> Self {
        match *self {
            Self::Enabled {
                toggle_override,
                setting_configuration,
            } => Self::Enabled {
                setting_configuration,
                toggle_override: !toggle_override,
            },
            Self::Disabled => Self::Disabled,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct BreadcrumbsVisibility {
    setting_configuration: bool,
    toggle_override: bool,
}

impl BreadcrumbsVisibility {
    pub(crate) fn from_settings(cx: &App) -> Self {
        Self::new(EditorSettings::get_global(cx).toolbar.breadcrumbs)
    }

    pub(crate) fn new(setting_configuration: bool) -> Self {
        Self {
            setting_configuration,
            toggle_override: false,
        }
    }

    pub(crate) fn settings_visibility(&self) -> bool {
        self.setting_configuration
    }

    pub(crate) fn visible(&self) -> bool {
        self.setting_configuration ^ self.toggle_override
    }

    pub(crate) fn toggle_visibility(&self) -> Self {
        Self {
            setting_configuration: self.setting_configuration,
            toggle_override: !self.toggle_override,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BufferSerialization {
    All,
    NonDirtyBuffers,
}

impl BufferSerialization {
    pub(crate) fn new(restore_unsaved_buffers: bool) -> Self {
        if restore_unsaved_buffers {
            Self::All
        } else {
            Self::NonDirtyBuffers
        }
    }
}

pub(crate) type CompletionId = usize;

pub struct ContextMenuOptions {
    pub min_entries_visible: usize,
    pub max_entries_visible: usize,
    pub placement: Option<ContextMenuPlacement>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextMenuPlacement {
    Above,
    Below,
}

#[derive(Copy, Clone, Eq, PartialEq, PartialOrd, Ord, Debug, Default)]
pub(crate) struct EditorActionId(usize);

impl EditorActionId {
    pub(crate) fn post_inc(&mut self) -> Self {
        let answer = self.0;

        *self = Self(answer + 1);

        Self(answer)
    }
}

#[derive(Clone)]
pub struct EditorStyle {
    pub background: Hsla,
    pub border: Hsla,
    pub local_player: PlayerColor,
    pub text: TextStyle,
    pub scrollbar_width: Pixels,
    pub syntax: Arc<SyntaxTheme>,
    pub status: StatusColors,
    pub inlay_hints_style: HighlightStyle,
    pub edit_prediction_styles: EditPredictionStyles,
    pub unnecessary_code_fade: f32,
    pub show_underlines: bool,
}

impl Default for EditorStyle {
    fn default() -> Self {
        static NONE_SYNTAX: std::sync::LazyLock<Arc<SyntaxTheme>> =
            std::sync::LazyLock::new(|| Arc::new(SyntaxTheme::default()));
        Self {
            background: Hsla::default(),
            border: Hsla::default(),
            local_player: PlayerColor::default(),
            text: TextStyle::default(),
            scrollbar_width: Pixels::default(),
            syntax: NONE_SYNTAX.clone(),
            // HACK: Status colors don't have a real default.
            // We should look into removing the status colors from the editor
            // style and retrieve them directly from the theme.
            status: StatusColors::dark(),
            inlay_hints_style: HighlightStyle::default(),
            edit_prediction_styles: EditPredictionStyles {
                insertion: HighlightStyle::default(),
                whitespace: HighlightStyle::default(),
            },
            unnecessary_code_fade: Default::default(),
            show_underlines: true,
        }
    }
}

pub fn make_inlay_hints_style(cx: &App) -> HighlightStyle {
    let show_background = AllLanguageSettings::get_global(cx)
        .defaults
        .inlay_hints
        .show_background;

    let mut style = cx
        .theme()
        .syntax()
        .style_for_name("hint")
        .unwrap_or_default();

    if style.color.is_none() {
        style.color = Some(cx.theme().status().hint);
    }

    if !show_background {
        style.background_color = None;
        return style;
    }

    if style.background_color.is_none() {
        style.background_color = Some(cx.theme().status().hint_background);
    }

    style
}
