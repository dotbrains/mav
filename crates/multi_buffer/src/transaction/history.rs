use gpui::{Context, Entity};
use language::{self, Buffer, TransactionId};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
use text::BufferId;

use crate::MultiBuffer;

#[derive(Clone)]
pub(crate) struct History {
    pub(super) undo_stack: Vec<Transaction>,
    next_transaction_id: TransactionId,
    redo_stack: Vec<Transaction>,
    transaction_depth: usize,
    group_interval: Duration,
}

impl Default for History {
    fn default() -> Self {
        History {
            undo_stack: Vec::new(),
            next_transaction_id: clock::Lamport::MIN,
            redo_stack: Vec::new(),
            transaction_depth: 0,
            group_interval: Duration::from_millis(300),
        }
    }
}

#[derive(Clone)]
pub(super) struct Transaction {
    pub(super) id: TransactionId,
    pub(super) buffer_transactions: HashMap<BufferId, text::TransactionId>,
    first_edit_at: Instant,
    last_edit_at: Instant,
    suppress_grouping: bool,
}

impl History {
    pub(super) fn start_transaction(&mut self, now: Instant) -> Option<TransactionId> {
        self.transaction_depth += 1;
        if self.transaction_depth == 1 {
            let id = self.next_transaction_id.tick();
            self.undo_stack.push(Transaction {
                id,
                buffer_transactions: Default::default(),
                first_edit_at: now,
                last_edit_at: now,
                suppress_grouping: false,
            });
            Some(id)
        } else {
            None
        }
    }

    pub(super) fn end_transaction(
        &mut self,
        now: Instant,
        buffer_transactions: HashMap<BufferId, text::TransactionId>,
    ) -> bool {
        assert_ne!(self.transaction_depth, 0);
        self.transaction_depth -= 1;
        if self.transaction_depth == 0 {
            if buffer_transactions.is_empty() {
                self.undo_stack.pop();
                false
            } else {
                self.redo_stack.clear();
                let transaction = self.undo_stack.last_mut().unwrap();
                transaction.last_edit_at = now;
                for (buffer_id, transaction_id) in buffer_transactions {
                    transaction
                        .buffer_transactions
                        .entry(buffer_id)
                        .or_insert(transaction_id);
                }
                true
            }
        } else {
            false
        }
    }

    pub(super) fn push_transaction<'a, T>(
        &mut self,
        buffer_transactions: T,
        now: Instant,
        cx: &Context<MultiBuffer>,
    ) where
        T: IntoIterator<Item = (&'a Entity<Buffer>, &'a language::Transaction)>,
    {
        assert_eq!(self.transaction_depth, 0);
        let transaction = Transaction {
            id: self.next_transaction_id.tick(),
            buffer_transactions: buffer_transactions
                .into_iter()
                .map(|(buffer, transaction)| (buffer.read(cx).remote_id(), transaction.id))
                .collect(),
            first_edit_at: now,
            last_edit_at: now,
            suppress_grouping: false,
        };
        if !transaction.buffer_transactions.is_empty() {
            self.undo_stack.push(transaction);
            self.redo_stack.clear();
        }
    }

    pub(super) fn finalize_last_transaction(&mut self) {
        if let Some(transaction) = self.undo_stack.last_mut() {
            transaction.suppress_grouping = true;
        }
    }

    pub(super) fn forget(&mut self, transaction_id: TransactionId) -> Option<Transaction> {
        if let Some(ix) = self
            .undo_stack
            .iter()
            .rposition(|transaction| transaction.id == transaction_id)
        {
            Some(self.undo_stack.remove(ix))
        } else if let Some(ix) = self
            .redo_stack
            .iter()
            .rposition(|transaction| transaction.id == transaction_id)
        {
            Some(self.redo_stack.remove(ix))
        } else {
            None
        }
    }

    pub(super) fn transaction(&self, transaction_id: TransactionId) -> Option<&Transaction> {
        self.undo_stack
            .iter()
            .find(|transaction| transaction.id == transaction_id)
            .or_else(|| {
                self.redo_stack
                    .iter()
                    .find(|transaction| transaction.id == transaction_id)
            })
    }

    pub(super) fn transaction_mut(
        &mut self,
        transaction_id: TransactionId,
    ) -> Option<&mut Transaction> {
        self.undo_stack
            .iter_mut()
            .find(|transaction| transaction.id == transaction_id)
            .or_else(|| {
                self.redo_stack
                    .iter_mut()
                    .find(|transaction| transaction.id == transaction_id)
            })
    }

    pub(super) fn pop_undo(&mut self) -> Option<&mut Transaction> {
        assert_eq!(self.transaction_depth, 0);
        if let Some(transaction) = self.undo_stack.pop() {
            self.redo_stack.push(transaction);
            self.redo_stack.last_mut()
        } else {
            None
        }
    }

    pub(super) fn pop_redo(&mut self) -> Option<&mut Transaction> {
        assert_eq!(self.transaction_depth, 0);
        if let Some(transaction) = self.redo_stack.pop() {
            self.undo_stack.push(transaction);
            self.undo_stack.last_mut()
        } else {
            None
        }
    }

    pub(super) fn remove_from_undo(
        &mut self,
        transaction_id: TransactionId,
    ) -> Option<&Transaction> {
        let ix = self
            .undo_stack
            .iter()
            .rposition(|transaction| transaction.id == transaction_id)?;
        let transaction = self.undo_stack.remove(ix);
        self.redo_stack.push(transaction);
        self.redo_stack.last()
    }

    pub(super) fn group(&mut self) -> Option<TransactionId> {
        let mut count = 0;
        let mut transactions = self.undo_stack.iter();
        if let Some(mut transaction) = transactions.next_back() {
            while let Some(prev_transaction) = transactions.next_back() {
                if !prev_transaction.suppress_grouping
                    && transaction.first_edit_at - prev_transaction.last_edit_at
                        <= self.group_interval
                {
                    transaction = prev_transaction;
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
        for transaction in self.undo_stack.iter().rev() {
            if transaction.id == transaction_id {
                self.group_trailing(count);
                break;
            } else if transaction.suppress_grouping {
                break;
            } else {
                count += 1;
            }
        }
    }

    fn group_trailing(&mut self, n: usize) -> Option<TransactionId> {
        let new_len = self.undo_stack.len() - n;
        let (transactions_to_keep, transactions_to_merge) = self.undo_stack.split_at_mut(new_len);
        if let Some(last_transaction) = transactions_to_keep.last_mut() {
            if let Some(transaction) = transactions_to_merge.last() {
                last_transaction.last_edit_at = transaction.last_edit_at;
            }
            for to_merge in transactions_to_merge {
                for (buffer_id, transaction_id) in &to_merge.buffer_transactions {
                    last_transaction
                        .buffer_transactions
                        .entry(*buffer_id)
                        .or_insert(*transaction_id);
                }
            }
        }

        self.undo_stack.truncate(new_len);
        self.undo_stack.last().map(|t| t.id)
    }

    pub(crate) fn transaction_depth(&self) -> usize {
        self.transaction_depth
    }

    pub fn set_group_interval(&mut self, group_interval: Duration) {
        self.group_interval = group_interval;
    }
}
