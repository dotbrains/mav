use crate::{
    ActiveDebugLine, Anchor, Autoscroll, BufferSerialization, Capability, Editor, EditorEvent,
    EditorSettings, ExcerptRange, FormatTarget, MultiBuffer, MultiBufferSnapshot, NavigationData,
    ReportEditorEvent, SelectionEffects, ToPoint as _,
    display_map::HighlightKey,
    editor_settings::SeedQuerySetting,
    persistence::{EditorDb, SerializedEditor},
    scroll::{ScrollAnchor, ScrollOffset},
};
use anyhow::{Context as _, Result, anyhow};
use collections::{HashMap, HashSet};
use file_icons::FileIcons;
use fs::MTime;
use futures::{channel::oneshot, future::try_join_all};
use git::status::GitSummary;
use gpui::{
    AnyElement, App, AsyncWindowContext, Context, Entity, EntityId, EventEmitter, Font,
    IntoElement, ParentElement, Pixels, SharedString, Styled, Task, WeakEntity, Window, point,
};
use language::{
    Bias, Buffer, BufferRow, CharKind, CharScopeContext, HighlightedText, LocalFile, Point,
    SelectionGoal, proto::serialize_anchor as serialize_text_anchor,
};
use lsp::DiagnosticSeverity;
use mav_actions::preview::{
    markdown::OpenPreview as OpenMarkdownPreview, svg::OpenPreview as OpenSvgPreview,
};
use multi_buffer::{BufferOffset, MultiBufferOffset, PathKey};
use project::{
    File, Project, ProjectItem as _, ProjectPath, lsp_store::FormatTrigger,
    project_settings::ProjectSettings, search::SearchQuery,
};
use rope::TextSummary;
use rpc::proto::{self, update_view};
use settings::Settings;
use std::{
    any::{Any, TypeId},
    borrow::Cow,
    cmp::{self, Ordering},
    num::NonZeroU32,
    ops::Range,
    path::{Path, PathBuf},
    sync::Arc,
};
use text::{BufferId, BufferSnapshot, OffsetRangeExt, Selection};
use ui::{IconDecorationKind, prelude::*};
use util::{ResultExt, TryFutureExt, paths::PathExt, rel_path::RelPath};
use workspace::item::{Dedup, ItemSettings, SerializableItem, TabContentParams, tab_label_color};
use workspace::{
    CollaboratorId, ItemId, ItemNavHistory, ToolbarItemLocation, ViewId, Workspace, WorkspaceId,
    invalid_item_view::InvalidItemView,
    item::{FollowableItem, Item, ItemBufferKind, ItemEvent, ProjectItem, SaveOptions},
    searchable::{
        Direction, FilteredSearchRange, SearchEvent, SearchToken, SearchableItem,
        SearchableItemHandle,
    },
};
use workspace::{
    Pane, TabBarSettings, WorkspaceSettings,
    item::{FollowEvent, ProjectItemKind},
    searchable::SearchOptions,
};

pub const MAX_TAB_TITLE_LEN: usize = 24;

mod follow;
mod helpers;
mod item;
mod project_item;
mod protocol;
mod restoration;
mod search;
mod serialization;
#[cfg(test)]
mod tests;

use helpers::*;
pub(crate) use helpers::{
    chunk_search_range, deserialize_path_key, path_for_buffer, restore_serialized_buffer_contents,
    serialize_path_key,
};
pub use helpers::{
    entry_diagnostic_aware_icon_decoration_and_color, entry_diagnostic_aware_icon_name_and_color,
    entry_git_aware_label_color, entry_label_color,
};
pub(crate) use project_item::{EditorRestorationData, RestorationData};
use protocol::*;
pub use search::active_match_index;
