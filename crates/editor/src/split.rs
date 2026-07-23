use std::{
    ops::{Range, RangeInclusive},
    sync::Arc,
};

use buffer_diff::{BufferDiff, BufferDiffSnapshot};
use collections::HashMap;

use gpui::{
    Action, AppContext as _, Entity, EventEmitter, Focusable, Font, Pixels, Subscription,
    WeakEntity, canvas,
};
use itertools::Itertools;
use language::{Buffer, Capability, HighlightedText};
use multi_buffer::{
    Anchor, AnchorRangeExt as _, BufferOffset, ExcerptRange, ExpandExcerptDirection, MultiBuffer,
    MultiBufferDiffHunk, MultiBufferPoint, MultiBufferSnapshot, PathKey,
};
use project::Project;
use rope::Point;
use settings::{DiffViewStyle, SeedQuerySetting, Settings, SettingsStore};
use text::{Bias, BufferId, OffsetRangeExt as _, Patch, ToPoint as _};
use ui::{
    App, Context, InteractiveElement as _, IntoElement as _, ParentElement as _, Render,
    Styled as _, Window, div,
};

use crate::{
    display_map::CompanionExcerptPatch,
    element::SplitSide,
    split_editor_view::{SplitEditorState, SplitEditorView},
};
use workspace::{
    ActivatePaneLeft, ActivatePaneRight, Item, ToolbarItemLocation, Workspace,
    item::{ItemBufferKind, ItemEvent, SaveOptions, TabContentParams},
    searchable::{SearchEvent, SearchToken, SearchableItem, SearchableItemHandle},
};

use crate::{
    Autoscroll, Editor, EditorEvent, EditorSettings, RenderDiffHunkControlsFn, ToggleSoftWrap,
    actions::{DisableBreakpoint, EditLogBreakpoint, EnableBreakpoint, ToggleBreakpoint},
    display_map::Companion,
};
use mav_actions::assistant::InlineAssist;

mod patches;
use patches::{
    buffer_range_to_base_text_range, translate_lhs_hunks_to_rhs, translate_lhs_selections_to_rhs,
};
pub(crate) use patches::{patches_for_lhs_range, patches_for_rhs_range};

#[derive(Clone, Copy, PartialEq, Eq, Action, Default)]
#[action(namespace = editor)]
pub struct ToggleSplitDiff;

pub struct SplittableEditor {
    rhs_multibuffer: Entity<MultiBuffer>,
    rhs_editor: Entity<Editor>,
    lhs: Option<LhsEditor>,
    workspace: WeakEntity<Workspace>,
    split_state: Entity<SplitEditorState>,
    searched_side: Option<SplitSide>,
    /// The preferred diff style.
    diff_view_style: DiffViewStyle,
    /// True when the current width is below the minimum threshold for split
    /// mode, regardless of the current diff view style setting.
    too_narrow_for_split: bool,
    last_width: Option<Pixels>,
    _subscriptions: Vec<Subscription>,
}

struct LhsEditor {
    multibuffer: Entity<MultiBuffer>,
    editor: Entity<Editor>,
    was_last_focused: bool,
    _subscriptions: Vec<Subscription>,
}

mod actions;
mod core;
#[cfg(test)]
mod debug;
mod excerpts;
#[cfg(test)]
mod invariants;
mod item;
mod render;
mod search;
mod split_mode;

#[cfg(test)]
mod tests;
