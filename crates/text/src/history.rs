use super::*;

#[derive(Clone, Debug)]
pub struct HistoryEntry {
    pub(super) transaction: Transaction,
    pub(super) first_edit_at: Instant,
    pub(super) last_edit_at: Instant,
    pub(super) suppress_grouping: bool,
}

#[derive(Clone, Debug)]
pub struct Transaction {
    pub id: TransactionId,
    pub edit_ids: Vec<clock::Lamport>,
    pub start: clock::Global,
}

impl Transaction {
    pub fn merge_in(&mut self, other: Transaction) {
        self.edit_ids.extend(other.edit_ids);
    }
}

impl HistoryEntry {
    pub fn transaction_id(&self) -> TransactionId {
        self.transaction.id
    }
}

pub(super) struct History {
    pub(super) base_text: Rope,
    pub(super) operations: TreeMap<clock::Lamport, Operation>,
    pub(super) undo_stack: Vec<HistoryEntry>,
    pub(super) redo_stack: Vec<HistoryEntry>,
    transaction_depth: usize,
    pub(super) group_interval: Duration,
}

impl History {
    pub fn new(base_text: Rope) -> Self {
        Self {
            base_text,
            operations: Default::default(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            transaction_depth: 0,
            // Don't group transactions in tests unless we opt in, because it's a footgun.
            group_interval: if cfg!(any(test, feature = "test-support")) {
                Duration::ZERO
            } else {
                Duration::from_millis(300)
            },
        }
    }

    pub(super) fn push(&mut self, op: Operation) {
        self.operations.insert(op.timestamp(), op);
    }

    pub(super) fn start_transaction(
        &mut self,
        start: clock::Global,
        now: Instant,
        clock: &mut clock::Lamport,
    ) -> Option<TransactionId> {
        self.transaction_depth += 1;
        if self.transaction_depth == 1 {
            let id = clock.tick();
            self.undo_stack.push(HistoryEntry {
                transaction: Transaction {
                    id,
                    start,
                    edit_ids: Default::default(),
                },
                first_edit_at: now,
                last_edit_at: now,
                suppress_grouping: false,
            });
            Some(id)
        } else {
            None
        }
    }

    pub(super) fn end_transaction(&mut self, now: Instant) -> Option<&HistoryEntry> {
        assert_ne!(self.transaction_depth, 0);
        self.transaction_depth -= 1;
        if self.transaction_depth == 0 {
            if self
                .undo_stack
                .last()
                .unwrap()
                .transaction
                .edit_ids
                .is_empty()
            {
                self.undo_stack.pop();
                None
            } else {
                self.redo_stack.clear();
                let entry = self.undo_stack.last_mut().unwrap();
                entry.last_edit_at = now;
                Some(entry)
            }
        } else {
            None
        }
    }

    pub(super) fn group(&mut self) -> Option<TransactionId> {
        let mut count = 0;
        let mut entries = self.undo_stack.iter();
        if let Some(mut entry) = entries.next_back() {
            while let Some(prev_entry) = entries.next_back() {
                if !prev_entry.suppress_grouping
                    && entry.first_edit_at - prev_entry.last_edit_at < self.group_interval
                {
                    entry = prev_entry;
                    count += 1;
                } else {
                    break;
                }
            }
        }
        self.group_trailing(count)
    }

    pub(super) fn group_until(&mut self, transaction_id: TransactionId) {
        let mut count = 0;
        for entry in self.undo_stack.iter().rev() {
            if entry.transaction_id() == transaction_id {
                self.group_trailing(count);
                break;
            } else if entry.suppress_grouping {
                break;
            } else {
                count += 1;
            }
        }
    }

    pub(super) fn group_trailing(&mut self, n: usize) -> Option<TransactionId> {
        let new_len = self.undo_stack.len() - n;
        let (entries_to_keep, entries_to_merge) = self.undo_stack.split_at_mut(new_len);
        if let Some(last_entry) = entries_to_keep.last_mut() {
            for entry in &*entries_to_merge {
                for edit_id in &entry.transaction.edit_ids {
                    last_entry.transaction.edit_ids.push(*edit_id);
                }
            }

            if let Some(entry) = entries_to_merge.last_mut() {
                last_entry.last_edit_at = entry.last_edit_at;
            }
        }

        self.undo_stack.truncate(new_len);
        self.undo_stack.last().map(|e| e.transaction.id)
    }

    pub(super) fn finalize_last_transaction(&mut self) -> Option<&Transaction> {
        self.undo_stack.last_mut().map(|entry| {
            entry.transaction.edit_ids.shrink_to_fit();
            entry.suppress_grouping = true;
            &entry.transaction
        })
    }

    pub(super) fn push_transaction(&mut self, transaction: Transaction, now: Instant) {
        assert_eq!(self.transaction_depth, 0);
        self.undo_stack.push(HistoryEntry {
            transaction,
            first_edit_at: now,
            last_edit_at: now,
            suppress_grouping: false,
        });
    }

    /// Differs from `push_transaction` in that it does not clear the redo
    /// stack. Intended to be used to create a parent transaction to merge
    /// potential child transactions into.
    ///
    /// The caller is responsible for removing it from the undo history using
    /// `forget_transaction` if no edits are merged into it. Otherwise, if edits
    /// are merged into this transaction, the caller is responsible for ensuring
    /// the redo stack is cleared. The easiest way to ensure the redo stack is
    /// cleared is to create transactions with the usual `start_transaction` and
    /// `end_transaction` methods and merging the resulting transactions into
    /// the transaction created by this method
    pub(super) fn push_empty_transaction(
        &mut self,
        start: clock::Global,
        now: Instant,
        clock: &mut clock::Lamport,
    ) -> TransactionId {
        assert_eq!(self.transaction_depth, 0);
        let id = clock.tick();
        let transaction = Transaction {
            id,
            start,
            edit_ids: Vec::new(),
        };
        self.undo_stack.push(HistoryEntry {
            transaction,
            first_edit_at: now,
            last_edit_at: now,
            suppress_grouping: false,
        });
        id
    }

    pub(super) fn push_undo(&mut self, op_id: clock::Lamport) {
        assert_ne!(self.transaction_depth, 0);
        if let Some(Operation::Edit(_)) = self.operations.get(&op_id) {
            let last_transaction = self.undo_stack.last_mut().unwrap();
            last_transaction.transaction.edit_ids.push(op_id);
        }
    }

    pub(super) fn pop_undo(&mut self) -> Option<&HistoryEntry> {
        assert_eq!(self.transaction_depth, 0);
        if let Some(entry) = self.undo_stack.pop() {
            self.redo_stack.push(entry);
            self.redo_stack.last()
        } else {
            None
        }
    }

    pub(super) fn remove_from_undo(
        &mut self,
        transaction_id: TransactionId,
    ) -> Option<&HistoryEntry> {
        assert_eq!(self.transaction_depth, 0);

        let entry_ix = self
            .undo_stack
            .iter()
            .rposition(|entry| entry.transaction.id == transaction_id)?;
        let entry = self.undo_stack.remove(entry_ix);
        self.redo_stack.push(entry);
        self.redo_stack.last()
    }

    pub(super) fn remove_from_undo_until(
        &mut self,
        transaction_id: TransactionId,
    ) -> &[HistoryEntry] {
        assert_eq!(self.transaction_depth, 0);

        let redo_stack_start_len = self.redo_stack.len();
        if let Some(entry_ix) = self
            .undo_stack
            .iter()
            .rposition(|entry| entry.transaction.id == transaction_id)
        {
            self.redo_stack
                .extend(self.undo_stack.drain(entry_ix..).rev());
        }
        &self.redo_stack[redo_stack_start_len..]
    }

    pub(super) fn forget(&mut self, transaction_id: TransactionId) -> Option<Transaction> {
        assert_eq!(self.transaction_depth, 0);
        if let Some(entry_ix) = self
            .undo_stack
            .iter()
            .rposition(|entry| entry.transaction.id == transaction_id)
        {
            Some(self.undo_stack.remove(entry_ix).transaction)
        } else if let Some(entry_ix) = self
            .redo_stack
            .iter()
            .rposition(|entry| entry.transaction.id == transaction_id)
        {
            Some(self.redo_stack.remove(entry_ix).transaction)
        } else {
            None
        }
    }

    pub(super) fn transaction(&self, transaction_id: TransactionId) -> Option<&Transaction> {
        let entry = self
            .undo_stack
            .iter()
            .rfind(|entry| entry.transaction.id == transaction_id)
            .or_else(|| {
                self.redo_stack
                    .iter()
                    .rfind(|entry| entry.transaction.id == transaction_id)
            })?;
        Some(&entry.transaction)
    }

    pub(super) fn transaction_mut(
        &mut self,
        transaction_id: TransactionId,
    ) -> Option<&mut Transaction> {
        let entry = self
            .undo_stack
            .iter_mut()
            .rfind(|entry| entry.transaction.id == transaction_id)
            .or_else(|| {
                self.redo_stack
                    .iter_mut()
                    .rfind(|entry| entry.transaction.id == transaction_id)
            })?;
        Some(&mut entry.transaction)
    }

    pub(super) fn merge_transactions(
        &mut self,
        transaction: TransactionId,
        destination: TransactionId,
    ) {
        if let Some(transaction) = self.forget(transaction)
            && let Some(destination) = self.transaction_mut(destination)
        {
            destination.edit_ids.extend(transaction.edit_ids);
        }
    }

    pub(super) fn pop_redo(&mut self) -> Option<&HistoryEntry> {
        assert_eq!(self.transaction_depth, 0);
        if let Some(entry) = self.redo_stack.pop() {
            self.undo_stack.push(entry);
            self.undo_stack.last()
        } else {
            None
        }
    }

    pub(super) fn remove_from_redo(&mut self, transaction_id: TransactionId) -> &[HistoryEntry] {
        assert_eq!(self.transaction_depth, 0);

        let undo_stack_start_len = self.undo_stack.len();
        if let Some(entry_ix) = self
            .redo_stack
            .iter()
            .rposition(|entry| entry.transaction.id == transaction_id)
        {
            self.undo_stack
                .extend(self.redo_stack.drain(entry_ix..).rev());
        }
        &self.undo_stack[undo_stack_start_len..]
    }
}
