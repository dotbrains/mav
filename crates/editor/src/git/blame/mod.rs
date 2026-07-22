use crate::Editor;
use anyhow::{Context as _, Result};
use collections::HashMap;

use git::{
    GitHostingProviderRegistry, Oid,
    blame::{Blame, BlameEntry},
    commit::ParsedCommitMessage,
};
use gpui::{
    AnyElement, App, AppContext as _, Context, Entity, Hsla, ScrollHandle, Subscription, Task,
    TextStyle, WeakEntity, Window,
};
use itertools::Itertools;
use language::{Bias, BufferSnapshot, Edit};
use markdown::Markdown;
use multi_buffer::{MultiBuffer, RowInfo};
use project::{
    Project, ProjectItem as _,
    git_store::{GitStoreEvent, Repository},
};
use smallvec::SmallVec;
use std::{sync::Arc, time::Duration};
use sum_tree::SumTree;
use text::BufferId;
use workspace::Workspace;

#[derive(Clone, Debug, Default)]
pub struct GitBlameEntry {
    pub rows: u32,
    pub blame: Option<BlameEntry>,
}

#[derive(Clone, Debug, Default)]
pub struct GitBlameEntrySummary {
    rows: u32,
}

impl sum_tree::Item for GitBlameEntry {
    type Summary = GitBlameEntrySummary;

    fn summary(&self, _cx: ()) -> Self::Summary {
        GitBlameEntrySummary { rows: self.rows }
    }
}

impl sum_tree::ContextLessSummary for GitBlameEntrySummary {
    fn zero() -> Self {
        Default::default()
    }

    fn add_summary(&mut self, summary: &Self) {
        self.rows += summary.rows;
    }
}

impl<'a> sum_tree::Dimension<'a, GitBlameEntrySummary> for u32 {
    fn zero(_cx: ()) -> Self {
        Default::default()
    }

    fn add_summary(&mut self, summary: &'a GitBlameEntrySummary, _cx: ()) {
        *self += summary.rows;
    }
}

struct GitBlameBuffer {
    entries: SumTree<GitBlameEntry>,
    buffer_snapshot: BufferSnapshot,
    buffer_edits: text::Subscription<usize>,
    commit_details: HashMap<Oid, ParsedCommitMessage>,
}

pub struct GitBlame {
    project: Entity<Project>,
    multi_buffer: WeakEntity<MultiBuffer>,
    buffers: HashMap<BufferId, GitBlameBuffer>,
    task: Task<Result<()>>,
    focused: bool,
    changed_while_blurred: bool,
    user_triggered: bool,
    regenerate_on_edit_task: Task<Result<()>>,
    _regenerate_subscriptions: Vec<Subscription>,
}

pub trait BlameRenderer {
    fn max_author_length(&self) -> usize;

    fn render_blame_entry(
        &self,
        _: &TextStyle,
        _: BlameEntry,
        _: Option<ParsedCommitMessage>,
        _: Entity<Repository>,
        _: WeakEntity<Workspace>,
        _: Entity<Editor>,
        _: usize,
        _: Hsla,
        window: &mut Window,
        _: &mut App,
    ) -> Option<AnyElement>;

    fn render_inline_blame_entry(
        &self,
        _: &TextStyle,
        _: BlameEntry,
        _: &mut App,
    ) -> Option<AnyElement>;

    fn render_blame_entry_popover(
        &self,
        _: BlameEntry,
        _: ScrollHandle,
        _: Option<ParsedCommitMessage>,
        _: Entity<Markdown>,
        _: Entity<Repository>,
        _: WeakEntity<Workspace>,
        _: &mut Window,
        _: &mut App,
    ) -> Option<AnyElement>;

    fn open_blame_commit(
        &self,
        _: BlameEntry,
        _: Entity<Repository>,
        _: WeakEntity<Workspace>,
        _: &mut Window,
        _: &mut App,
    );
}

impl BlameRenderer for () {
    fn max_author_length(&self) -> usize {
        0
    }

    fn render_blame_entry(
        &self,
        _: &TextStyle,
        _: BlameEntry,
        _: Option<ParsedCommitMessage>,
        _: Entity<Repository>,
        _: WeakEntity<Workspace>,
        _: Entity<Editor>,
        _: usize,
        _: Hsla,
        _: &mut Window,
        _: &mut App,
    ) -> Option<AnyElement> {
        None
    }

    fn render_inline_blame_entry(
        &self,
        _: &TextStyle,
        _: BlameEntry,
        _: &mut App,
    ) -> Option<AnyElement> {
        None
    }

    fn render_blame_entry_popover(
        &self,
        _: BlameEntry,
        _: ScrollHandle,
        _: Option<ParsedCommitMessage>,
        _: Entity<Markdown>,
        _: Entity<Repository>,
        _: WeakEntity<Workspace>,
        _: &mut Window,
        _: &mut App,
    ) -> Option<AnyElement> {
        None
    }

    fn open_blame_commit(
        &self,
        _: BlameEntry,
        _: Entity<Repository>,
        _: WeakEntity<Workspace>,
        _: &mut Window,
        _: &mut App,
    ) {
    }
}

pub(crate) struct GlobalBlameRenderer(pub Arc<dyn BlameRenderer>);

impl gpui::Global for GlobalBlameRenderer {}

mod generate;
mod lifecycle;
mod sync;
#[cfg(test)]
mod tests;
mod tree;
