use super::*;

#[derive(Clone, Debug)]
pub(crate) struct SelectionHistoryEntry {
    pub(crate) selections: Arc<[Selection<Anchor>]>,
    pub(crate) select_next_state: Option<SelectNextState>,
    pub(crate) select_prev_state: Option<SelectNextState>,
    pub(crate) add_selections_state: Option<AddSelectionsState>,
}

#[derive(Copy, Clone, Default, Debug, PartialEq, Eq)]
pub(crate) enum SelectionHistoryMode {
    #[default]
    Normal,
    Undoing,
    Redoing,
    Skipping,
}

pub(crate) struct DeferredSelectionEffectsState {
    pub(crate) changed: bool,
    pub(crate) effects: SelectionEffects,
    pub(crate) old_cursor_position: Anchor,
    pub(crate) history_entry: SelectionHistoryEntry,
}

#[derive(Default)]
pub(crate) struct SelectionHistory {
    #[allow(clippy::type_complexity)]
    selections_by_transaction:
        HashMap<TransactionId, (Arc<[Selection<Anchor>]>, Option<Arc<[Selection<Anchor>]>>)>,
    pub(crate) mode: SelectionHistoryMode,
    pub(crate) undo_stack: VecDeque<SelectionHistoryEntry>,
    pub(crate) redo_stack: VecDeque<SelectionHistoryEntry>,
}

impl SelectionHistory {
    #[track_caller]
    pub(crate) fn insert_transaction(
        &mut self,
        transaction_id: TransactionId,
        selections: Arc<[Selection<Anchor>]>,
    ) {
        if selections.is_empty() {
            log::error!(
                "SelectionHistory::insert_transaction called with empty selections. Caller: {}",
                std::panic::Location::caller()
            );
            return;
        }
        self.selections_by_transaction
            .insert(transaction_id, (selections, None));
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn transaction(
        &self,
        transaction_id: TransactionId,
    ) -> Option<&(Arc<[Selection<Anchor>]>, Option<Arc<[Selection<Anchor>]>>)> {
        self.selections_by_transaction.get(&transaction_id)
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn transaction_mut(
        &mut self,
        transaction_id: TransactionId,
    ) -> Option<&mut (Arc<[Selection<Anchor>]>, Option<Arc<[Selection<Anchor>]>>)> {
        self.selections_by_transaction.get_mut(&transaction_id)
    }

    pub(crate) fn push(&mut self, entry: SelectionHistoryEntry) {
        if !entry.selections.is_empty() {
            match self.mode {
                SelectionHistoryMode::Normal => {
                    self.push_undo(entry);
                    self.redo_stack.clear();
                }
                SelectionHistoryMode::Undoing => self.push_redo(entry),
                SelectionHistoryMode::Redoing => self.push_undo(entry),
                SelectionHistoryMode::Skipping => {}
            }
        }
    }

    fn push_undo(&mut self, entry: SelectionHistoryEntry) {
        if self
            .undo_stack
            .back()
            .is_none_or(|e| e.selections != entry.selections)
        {
            self.undo_stack.push_back(entry);
            if self.undo_stack.len() > MAX_SELECTION_HISTORY_LEN {
                self.undo_stack.pop_front();
            }
        }
    }

    fn push_redo(&mut self, entry: SelectionHistoryEntry) {
        if self
            .redo_stack
            .back()
            .is_none_or(|e| e.selections != entry.selections)
        {
            self.redo_stack.push_back(entry);
            if self.redo_stack.len() > MAX_SELECTION_HISTORY_LEN {
                self.redo_stack.pop_front();
            }
        }
    }
}
