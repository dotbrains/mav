use super::*;

#[derive(Debug)]
pub(super) enum SelectedEntry {
    Invalidated(Option<PanelEntry>),
    Valid(PanelEntry, usize),
    None,
}

impl SelectedEntry {
    pub(super) fn invalidate(&mut self) {
        match std::mem::replace(self, SelectedEntry::None) {
            Self::Valid(entry, _) => *self = Self::Invalidated(Some(entry)),
            Self::None => *self = Self::Invalidated(None),
            other => *self = other,
        }
    }

    pub(super) fn is_invalidated(&self) -> bool {
        matches!(self, Self::Invalidated(_))
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct FsChildren {
    pub(super) files: usize,
    pub(super) dirs: usize,
}

impl FsChildren {
    pub(super) fn may_be_fold_part(&self) -> bool {
        self.dirs == 0 || (self.dirs == 1 && self.files == 0)
    }
}

#[derive(Clone, Debug)]
pub(super) struct CachedEntry {
    pub(super) depth: usize,
    pub(super) string_match: Option<StringMatch>,
    pub(super) entry: PanelEntry,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) enum CollapsedEntry {
    Dir(WorktreeId, ProjectEntryId),
    File(WorktreeId, BufferId),
    ExternalFile(BufferId),
    Excerpt(ExcerptRange<Anchor>),
    Outline(Range<Anchor>),
}

pub(super) struct BufferOutlines {
    pub(super) excerpts: Vec<ExcerptRange<Anchor>>,
    pub(super) outlines: OutlineState,
}

impl BufferOutlines {
    pub(super) fn invalidate_outlines(&mut self) {
        if let OutlineState::Outlines(valid_outlines) = &mut self.outlines {
            self.outlines = OutlineState::Invalidated(std::mem::take(valid_outlines));
        }
    }

    pub(super) fn iter_outlines(&self) -> impl Iterator<Item = &Outline> {
        match &self.outlines {
            OutlineState::Outlines(outlines) => outlines.iter(),
            OutlineState::Invalidated(outlines) => outlines.iter(),
            OutlineState::NotFetched => [].iter(),
        }
    }

    pub(super) fn should_fetch_outlines(&self) -> bool {
        match &self.outlines {
            OutlineState::Outlines(_) => false,
            OutlineState::Invalidated(_) => true,
            OutlineState::NotFetched => true,
        }
    }
}

#[derive(Debug)]
pub(super) enum OutlineState {
    Outlines(Vec<Outline>),
    Invalidated(Vec<Outline>),
    NotFetched,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct FoldedDirsEntry {
    pub(super) worktree_id: WorktreeId,
    pub(super) entries: Vec<GitEntry>,
}

// TODO: collapse the inner enums into panel entry
#[derive(Clone, Debug)]
pub(super) enum PanelEntry {
    Fs(FsEntry),
    FoldedDirs(FoldedDirsEntry),
    Outline(OutlineEntry),
    Search(SearchEntry),
}

#[derive(Clone, Debug)]
pub(super) struct SearchEntry {
    pub(super) match_range: Range<editor::Anchor>,
    pub(super) kind: SearchKind,
    pub(super) render_data: Arc<OnceLock<SearchData>>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub(super) enum SearchKind {
    Project,
    Buffer,
}

#[derive(Clone, Debug)]
pub(super) struct SearchData {
    pub(super) context_range: Range<editor::Anchor>,
    pub(super) context_text: String,
    pub(super) truncated_left: bool,
    pub(super) truncated_right: bool,
    pub(super) search_match_indices: Vec<Range<usize>>,
    pub(super) highlights_data: HighlightStyleData,
}

pub(super) struct SearchPrecomputed {
    pub(super) multi_buffer_snapshot: MultiBufferSnapshot,
    pub(super) matches_by_buffer:
        HashMap<BufferId, Vec<(Range<editor::Anchor>, Arc<OnceLock<SearchData>>)>>,
    pub(super) folded_buffers: HashSet<BufferId>,
}

impl PartialEq for PanelEntry {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Fs(a), Self::Fs(b)) => a == b,
            (
                Self::FoldedDirs(FoldedDirsEntry {
                    worktree_id: worktree_id_a,
                    entries: entries_a,
                }),
                Self::FoldedDirs(FoldedDirsEntry {
                    worktree_id: worktree_id_b,
                    entries: entries_b,
                }),
            ) => worktree_id_a == worktree_id_b && entries_a == entries_b,
            (Self::Outline(a), Self::Outline(b)) => a == b,
            (
                Self::Search(SearchEntry {
                    match_range: match_range_a,
                    kind: kind_a,
                    ..
                }),
                Self::Search(SearchEntry {
                    match_range: match_range_b,
                    kind: kind_b,
                    ..
                }),
            ) => match_range_a == match_range_b && kind_a == kind_b,
            _ => false,
        }
    }
}

impl Eq for PanelEntry {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum OutlineEntry {
    Excerpt(ExcerptRange<Anchor>),
    Outline(Outline),
}

impl OutlineEntry {
    pub(super) fn buffer_id(&self) -> BufferId {
        match self {
            OutlineEntry::Excerpt(excerpt) => excerpt.context.start.buffer_id,
            OutlineEntry::Outline(outline) => outline.range.start.buffer_id,
        }
    }

    pub(super) fn range(&self) -> Range<Anchor> {
        match self {
            OutlineEntry::Excerpt(excerpt) => excerpt.context.clone(),
            OutlineEntry::Outline(outline) => outline.range.clone(),
        }
    }
}

#[derive(Debug, Clone, Eq)]
pub(super) struct FsEntryFile {
    pub(super) worktree_id: WorktreeId,
    pub(super) entry: GitEntry,
    pub(super) buffer_id: BufferId,
    pub(super) excerpts: Vec<ExcerptRange<language::Anchor>>,
}

impl PartialEq for FsEntryFile {
    fn eq(&self, other: &Self) -> bool {
        self.worktree_id == other.worktree_id
            && self.entry.id == other.entry.id
            && self.buffer_id == other.buffer_id
    }
}

impl Hash for FsEntryFile {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        (self.buffer_id, self.entry.id, self.worktree_id).hash(state);
    }
}

#[derive(Debug, Clone, Eq)]
pub(super) struct FsEntryDirectory {
    pub(super) worktree_id: WorktreeId,
    pub(super) entry: GitEntry,
}

impl PartialEq for FsEntryDirectory {
    fn eq(&self, other: &Self) -> bool {
        self.worktree_id == other.worktree_id && self.entry.id == other.entry.id
    }
}

impl Hash for FsEntryDirectory {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        (self.worktree_id, self.entry.id).hash(state);
    }
}

#[derive(Debug, Clone, Eq)]
pub(super) struct FsEntryExternalFile {
    pub(super) buffer_id: BufferId,
    pub(super) excerpts: Vec<ExcerptRange<language::Anchor>>,
}

impl PartialEq for FsEntryExternalFile {
    fn eq(&self, other: &Self) -> bool {
        self.buffer_id == other.buffer_id
    }
}

impl Hash for FsEntryExternalFile {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.buffer_id.hash(state);
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum FsEntry {
    ExternalFile(FsEntryExternalFile),
    Directory(FsEntryDirectory),
    File(FsEntryFile),
}

pub(super) struct ActiveItem {
    pub(super) item_handle: Box<dyn WeakItemHandle>,
    pub(super) active_editor: WeakEntity<Editor>,
    pub(super) _buffer_search_subscription: Subscription,
    pub(super) _editor_subscription: Subscription,
}

#[derive(Debug)]
pub enum Event {
    Focus,
}

#[derive(Serialize, Deserialize)]
pub(super) struct SerializedOutlinePanel {
    pub(super) active: Option<bool>,
}
