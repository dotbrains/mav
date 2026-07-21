use super::*;

#[derive(Clone, Debug)]
pub(super) struct TraversalProgress<'a> {
    pub(super) max_path: &'a RelPath,
    count: usize,
    non_ignored_count: usize,
    file_count: usize,
    non_ignored_file_count: usize,
}

impl TraversalProgress<'_> {
    fn count(&self, include_files: bool, include_dirs: bool, include_ignored: bool) -> usize {
        match (include_files, include_dirs, include_ignored) {
            (true, true, true) => self.count,
            (true, true, false) => self.non_ignored_count,
            (true, false, true) => self.file_count,
            (true, false, false) => self.non_ignored_file_count,
            (false, true, true) => self.count - self.file_count,
            (false, true, false) => self.non_ignored_count - self.non_ignored_file_count,
            (false, false, _) => 0,
        }
    }
}

impl<'a> sum_tree::Dimension<'a, EntrySummary> for TraversalProgress<'a> {
    fn zero(_cx: ()) -> Self {
        Default::default()
    }

    fn add_summary(&mut self, summary: &'a EntrySummary, _: ()) {
        self.max_path = summary.max_path.as_ref();
        self.count += summary.count;
        self.non_ignored_count += summary.non_ignored_count;
        self.file_count += summary.file_count;
        self.non_ignored_file_count += summary.non_ignored_file_count;
    }
}

impl Default for TraversalProgress<'_> {
    fn default() -> Self {
        Self {
            max_path: RelPath::empty(),
            count: 0,
            non_ignored_count: 0,
            file_count: 0,
            non_ignored_file_count: 0,
        }
    }
}

#[derive(Debug)]
pub struct Traversal<'a> {
    pub(super) snapshot: &'a Snapshot,
    pub(super) cursor: sum_tree::Cursor<'a, 'static, Entry, TraversalProgress<'a>>,
    pub(super) include_ignored: bool,
    pub(super) include_files: bool,
    pub(super) include_dirs: bool,
}

impl<'a> Traversal<'a> {
    pub(super) fn new(
        snapshot: &'a Snapshot,
        include_files: bool,
        include_dirs: bool,
        include_ignored: bool,
        start_path: &RelPath,
    ) -> Self {
        let mut cursor = snapshot.entries_by_path.cursor(());
        cursor.seek(&TraversalTarget::path(start_path), Bias::Left);
        let mut traversal = Self {
            snapshot,
            cursor,
            include_files,
            include_dirs,
            include_ignored,
        };
        if traversal.end_offset() == traversal.start_offset() {
            traversal.next();
        }
        traversal
    }

    pub fn advance(&mut self) -> bool {
        self.advance_by(1)
    }

    pub fn advance_by(&mut self, count: usize) -> bool {
        self.cursor.seek_forward(
            &TraversalTarget::Count {
                count: self.end_offset() + count,
                include_dirs: self.include_dirs,
                include_files: self.include_files,
                include_ignored: self.include_ignored,
            },
            Bias::Left,
        )
    }

    pub fn advance_to_sibling(&mut self) -> bool {
        while let Some(entry) = self.cursor.item() {
            self.cursor
                .seek_forward(&TraversalTarget::successor(&entry.path), Bias::Left);
            if let Some(entry) = self.cursor.item()
                && (self.include_files || !entry.is_file())
                && (self.include_dirs || !entry.is_dir())
                && (self.include_ignored || !entry.is_ignored || entry.is_always_included)
            {
                return true;
            }
        }
        false
    }

    pub fn back_to_parent(&mut self) -> bool {
        let Some(parent_path) = self.cursor.item().and_then(|entry| entry.path.parent()) else {
            return false;
        };
        self.cursor
            .seek(&TraversalTarget::path(parent_path), Bias::Left)
    }

    pub fn entry(&self) -> Option<&'a Entry> {
        self.cursor.item()
    }

    pub fn snapshot(&self) -> &'a Snapshot {
        self.snapshot
    }

    pub fn start_offset(&self) -> usize {
        self.cursor
            .start()
            .count(self.include_files, self.include_dirs, self.include_ignored)
    }

    pub fn end_offset(&self) -> usize {
        self.cursor
            .end()
            .count(self.include_files, self.include_dirs, self.include_ignored)
    }
}

impl<'a> Iterator for Traversal<'a> {
    type Item = &'a Entry;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(item) = self.entry() {
            self.advance();
            Some(item)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum PathTarget<'a> {
    Path(&'a RelPath),
    Successor(&'a RelPath),
}

impl PathTarget<'_> {
    pub(super) fn cmp_path(&self, other: &RelPath) -> Ordering {
        match self {
            PathTarget::Path(path) => path.cmp(&other),
            PathTarget::Successor(path) => {
                if other.starts_with(path) {
                    Ordering::Greater
                } else {
                    Ordering::Equal
                }
            }
        }
    }
}

impl<'a, S: Summary> SeekTarget<'a, PathSummary<S>, PathProgress<'a>> for PathTarget<'_> {
    fn cmp(&self, cursor_location: &PathProgress<'a>, _: S::Context<'_>) -> Ordering {
        self.cmp_path(cursor_location.max_path)
    }
}

impl<'a, S: Summary> SeekTarget<'a, PathSummary<S>, TraversalProgress<'a>> for PathTarget<'_> {
    fn cmp(&self, cursor_location: &TraversalProgress<'a>, _: S::Context<'_>) -> Ordering {
        self.cmp_path(cursor_location.max_path)
    }
}

#[derive(Debug)]
pub(super) enum TraversalTarget<'a> {
    Path(PathTarget<'a>),
    Count {
        count: usize,
        include_files: bool,
        include_ignored: bool,
        include_dirs: bool,
    },
}

impl<'a> TraversalTarget<'a> {
    pub(super) fn path(path: &'a RelPath) -> Self {
        Self::Path(PathTarget::Path(path))
    }

    pub(super) fn successor(path: &'a RelPath) -> Self {
        Self::Path(PathTarget::Successor(path))
    }

    fn cmp_progress(&self, progress: &TraversalProgress) -> Ordering {
        match self {
            TraversalTarget::Path(path) => path.cmp_path(progress.max_path),
            TraversalTarget::Count {
                count,
                include_files,
                include_dirs,
                include_ignored,
            } => Ord::cmp(
                count,
                &progress.count(*include_files, *include_dirs, *include_ignored),
            ),
        }
    }
}

impl<'a> SeekTarget<'a, EntrySummary, TraversalProgress<'a>> for TraversalTarget<'_> {
    fn cmp(&self, cursor_location: &TraversalProgress<'a>, _: ()) -> Ordering {
        self.cmp_progress(cursor_location)
    }
}

impl<'a> SeekTarget<'a, PathSummary<sum_tree::NoSummary>, TraversalProgress<'a>>
    for TraversalTarget<'_>
{
    fn cmp(&self, cursor_location: &TraversalProgress<'a>, _: ()) -> Ordering {
        self.cmp_progress(cursor_location)
    }
}

pub struct ChildEntriesOptions {
    pub include_files: bool,
    pub include_dirs: bool,
    pub include_ignored: bool,
}

pub struct ChildEntriesIter<'a> {
    pub(super) parent_path: &'a RelPath,
    pub(super) traversal: Traversal<'a>,
}

impl<'a> Iterator for ChildEntriesIter<'a> {
    type Item = &'a Entry;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(item) = self.traversal.entry()
            && item.path.starts_with(self.parent_path)
        {
            self.traversal.advance_to_sibling();
            return Some(item);
        }
        None
    }
}

impl<'a> From<&'a Entry> for proto::Entry {
    fn from(entry: &'a Entry) -> Self {
        Self {
            id: entry.id.to_proto(),
            is_dir: entry.is_dir(),
            path: entry.path.as_ref().to_proto(),
            inode: entry.inode,
            mtime: entry.mtime.map(|time| time.into()),
            is_ignored: entry.is_ignored,
            is_hidden: entry.is_hidden,
            is_external: entry.is_external,
            is_fifo: entry.is_fifo,
            size: Some(entry.size),
            canonical_path: entry
                .canonical_path
                .as_ref()
                .map(|path| path.to_string_lossy().into_owned()),
        }
    }
}
