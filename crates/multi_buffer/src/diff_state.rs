use super::*;

pub(super) struct DiffState {
    pub(super) diff: Entity<BufferDiff>,
    main_buffer: Option<Entity<language::Buffer>>,
    _subscription: gpui::Subscription,
}

impl DiffState {
    pub(super) fn snapshot(&self, buffer_id: BufferId, cx: &App) -> DiffStateSnapshot {
        DiffStateSnapshot {
            buffer_id,
            diff: self.diff.read(cx).snapshot(cx),
            main_buffer: self.main_buffer.as_ref().map(|b| b.read(cx).snapshot()),
        }
    }
}

#[derive(Clone)]
pub(super) struct DiffStateSnapshot {
    pub(super) buffer_id: BufferId,
    pub(super) diff: BufferDiffSnapshot,
    pub(super) main_buffer: Option<language::BufferSnapshot>,
}

impl std::ops::Deref for DiffStateSnapshot {
    type Target = BufferDiffSnapshot;

    fn deref(&self) -> &Self::Target {
        &self.diff
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct DiffStateSummary {
    max_buffer_id: Option<BufferId>,
    pub(super) added_rows: u32,
    pub(super) removed_rows: u32,
}

impl sum_tree::ContextLessSummary for DiffStateSummary {
    fn zero() -> Self {
        Self::default()
    }

    fn add_summary(&mut self, other: &Self) {
        self.max_buffer_id = std::cmp::max(self.max_buffer_id, other.max_buffer_id);
        self.added_rows += other.added_rows;
        self.removed_rows += other.removed_rows;
    }
}

impl sum_tree::Item for DiffStateSnapshot {
    type Summary = DiffStateSummary;

    fn summary(&self, _cx: ()) -> DiffStateSummary {
        let (added_rows, removed_rows) = self.diff.changed_row_counts();
        DiffStateSummary {
            max_buffer_id: Some(self.buffer_id),
            added_rows,
            removed_rows,
        }
    }
}

impl sum_tree::KeyedItem for DiffStateSnapshot {
    type Key = Option<BufferId>;

    fn key(&self) -> Option<BufferId> {
        Some(self.buffer_id)
    }
}

impl<'a> Dimension<'a, DiffStateSummary> for Option<BufferId> {
    fn zero(_cx: ()) -> Self {
        None
    }

    fn add_summary(&mut self, summary: &DiffStateSummary, _cx: ()) {
        *self = std::cmp::max(*self, summary.max_buffer_id);
    }
}

pub(super) fn find_diff_state(
    diffs: &SumTree<DiffStateSnapshot>,
    buffer_id: BufferId,
) -> Option<&DiffStateSnapshot> {
    let key = Some(buffer_id);
    let (.., item) = diffs.find::<Option<BufferId>, _>((), &key, Bias::Left);
    item.filter(|entry| entry.buffer_id == buffer_id)
}

pub(super) fn remove_diff_state(diffs: &mut SumTree<DiffStateSnapshot>, buffer_id: BufferId) {
    let key = Some(buffer_id);
    let mut cursor = diffs.cursor::<Option<BufferId>>(());
    let mut new_tree = cursor.slice(&key, Bias::Left);
    if key == cursor.end() {
        cursor.next();
    }
    new_tree.append(cursor.suffix(), ());
    drop(cursor);
    *diffs = new_tree;
}

impl DiffState {
    pub(super) fn new(diff: Entity<BufferDiff>, cx: &mut Context<MultiBuffer>) -> Self {
        DiffState {
            _subscription: cx.subscribe(&diff, |this, diff, event, cx| match event {
                BufferDiffEvent::DiffChanged(DiffChanged {
                    changed_range,
                    base_text_changed_range: _,
                    extended_range,
                    base_text_changed: _,
                }) => {
                    let use_extended = this.snapshot.borrow().use_extended_diff_range;
                    let range = if use_extended {
                        extended_range.clone()
                    } else {
                        changed_range.clone()
                    };
                    this.buffer_diff_changed(diff, range, cx);
                    cx.emit(Event::BufferDiffChanged);
                }
                BufferDiffEvent::BaseTextChanged | BufferDiffEvent::HunksStagedOrUnstaged(_) => {}
            }),
            diff,
            main_buffer: None,
        }
    }

    pub(super) fn new_inverted(
        diff: Entity<BufferDiff>,
        main_buffer: Entity<language::Buffer>,
        cx: &mut Context<MultiBuffer>,
    ) -> Self {
        let weak_main_buffer = main_buffer.downgrade();
        DiffState {
            _subscription: cx.subscribe(&diff, {
                move |this, diff, event, cx| {
                    let Some(main_buffer) = weak_main_buffer.upgrade() else {
                        return;
                    };
                    match event {
                        BufferDiffEvent::DiffChanged(DiffChanged {
                            changed_range: _,
                            base_text_changed_range,
                            extended_range: _,
                            base_text_changed: _,
                        }) => {
                            this.inverted_buffer_diff_changed(
                                diff,
                                main_buffer,
                                base_text_changed_range.clone(),
                                cx,
                            );
                            cx.emit(Event::BufferDiffChanged);
                        }
                        BufferDiffEvent::BaseTextChanged
                        | BufferDiffEvent::HunksStagedOrUnstaged(_) => {}
                    }
                }
            }),
            diff,
            main_buffer: Some(main_buffer),
        }
    }
}
