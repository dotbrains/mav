use crate::{
    branch_picker, conflict_view,
    git_panel::{GitPanel, GitPanelAddon, GitStatusEntry},
    git_panel_settings::GitPanelSettings,
};
use agent_settings::AgentSettings;
use anyhow::{Context as _, Result, anyhow};
use buffer_diff::{BufferDiff, DiffHunkSecondaryStatus};
use collections::HashMap;
use editor::{
    Addon, Editor, EditorEvent, EditorSettings, SelectionEffects, SplittableEditor,
    actions::{GoToHunk, GoToPreviousHunk, SendReviewToAgent},
    multibuffer_context_lines,
    scroll::Autoscroll,
};
use futures_lite::future::yield_now;
use git::repository::DiffType;

use git::{
    Commit, StageAll, StageAndNext, ToggleStaged, UnstageAll, UnstageAndNext, repository::RepoPath,
    status::FileStatus,
};
use gpui::{
    Action, AnyElement, App, AppContext as _, AsyncWindowContext, Entity, EventEmitter,
    FocusHandle, Focusable, Render, Subscription, Task, WeakEntity, actions,
};
use language::{Anchor, Buffer, BufferId, Capability, OffsetRangeExt};
use mav_actions::agent::ReviewBranchDiff;
use multi_buffer::{MultiBuffer, PathKey};
use project::{
    ConflictSet, Project, ProjectPath,
    git_store::{
        Repository,
        branch_diff::{self, BranchDiffEvent, DiffBase},
    },
};
use settings::{GitPanelGroupBy, GitPanelSortBy, Settings, SettingsStore};
use std::any::{Any, TypeId};
use std::collections::BTreeMap;
use std::sync::Arc;
use theme::ActiveTheme;
use ui::{
    CommonAnimationExt as _, DiffStat, Divider, KeyBinding, PopoverMenu, Tooltip, prelude::*,
    vertical_divider,
};
use util::{ResultExt as _, rel_path::RelPath};
use workspace::{
    CloseActiveItem, ItemNavHistory, SerializableItem, ToolbarItemEvent, ToolbarItemLocation,
    ToolbarItemView, Workspace,
    item::{Item, ItemEvent, ItemHandle, SaveOptions, TabContentParams},
    notifications::NotifyTaskExt,
    searchable::SearchableItemHandle,
};
use ztracing::instrument;

actions!(
    git,
    [
        /// Shows the diff between the working directory and the index.
        Diff,
        /// Adds files to the git staging area.
        Add,
        /// Shows the diff between the working directory and your default
        /// branch (typically main or master).
        BranchDiff,
        /// Opens a new agent thread with the branch diff for review.
        ReviewDiff,
        LeaderAndFollower,
        /// Compare with a specific branch
        CompareWithBranch,
    ]
);

struct BufferSubscriptions {
    _diff: Entity<BufferDiff>,
    _diff_subscription: Subscription,
    _conflict_set: Entity<ConflictSet>,
    _conflict_set_subscription: Subscription,
}

pub struct ProjectDiff {
    project: Entity<Project>,
    multibuffer: Entity<MultiBuffer>,
    branch_diff: Entity<branch_diff::BranchDiff>,
    editor: Entity<SplittableEditor>,
    buffer_subscriptions: HashMap<Arc<RelPath>, BufferSubscriptions>,
    workspace: WeakEntity<Workspace>,
    focus_handle: FocusHandle,
    pending_scroll: Option<PathKey>,
    review_comment_count: usize,
    _task: Task<Result<()>>,
    _subscription: Subscription,
}

mod addon;
mod buffers;
mod construction;
mod deployment;
mod item;
mod navigation;
mod persistence;
mod render;
mod serialization;
mod sorting;
#[cfg(test)]
mod tests;
mod toolbar;

use toolbar::ButtonStates;
pub use toolbar::{BranchDiffToolbar, ProjectDiffToolbar};
