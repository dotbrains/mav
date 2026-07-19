use gpui::{App, Context, Entity};
use language::{self, Buffer, BufferEditSource, TransactionId};
use std::{collections::HashMap, ops::Range, time::Instant};
use sum_tree::Bias;

use crate::{Anchor, BufferState, MultiBufferOffset};

use super::{Event, MultiBuffer};

mod history;

pub(crate) use history::History;

impl MultiBuffer {
    pub fn start_transaction(&mut self, cx: &mut Context<Self>) -> Option<TransactionId> {
        self.start_transaction_at(Instant::now(), cx)
    }

    pub fn start_transaction_at(
        &mut self,
        now: Instant,
        cx: &mut Context<Self>,
    ) -> Option<TransactionId> {
        if let Some(buffer) = self.as_singleton() {
            return buffer.update(cx, |buffer, _| buffer.start_transaction_at(now));
        }

        for BufferState { buffer, .. } in self.buffers.values() {
            buffer.update(cx, |buffer, _| buffer.start_transaction_at(now));
        }
        self.history.start_transaction(now)
    }

    pub fn last_transaction_id(&self, cx: &App) -> Option<TransactionId> {
        if let Some(buffer) = self.as_singleton() {
            buffer
                .read(cx)
                .peek_undo_stack()
                .map(|history_entry| history_entry.transaction_id())
        } else {
            let last_transaction = self.history.undo_stack.last()?;
            Some(last_transaction.id)
        }
    }

    pub fn end_transaction(&mut self, cx: &mut Context<Self>) -> Option<TransactionId> {
        self.end_transaction_at(Instant::now(), cx)
    }

    pub fn end_transaction_with_source(
        &mut self,
        source: BufferEditSource,
        cx: &mut Context<Self>,
    ) -> Option<TransactionId> {
        let now = Instant::now();
        if let Some(buffer) = self.as_singleton() {
            return buffer.update(cx, |buffer, cx| {
                buffer.end_transaction_with_source(source, cx)
            });
        }

        let mut buffer_transactions = HashMap::default();
        for BufferState { buffer, .. } in self.buffers.values() {
            if let Some(transaction_id) = buffer.update(cx, |buffer, cx| {
                buffer.end_transaction_with_source(source, cx)
            }) {
                buffer_transactions.insert(buffer.read(cx).remote_id(), transaction_id);
            }
        }

        if self.history.end_transaction(now, buffer_transactions) {
            let transaction_id = self.history.group().unwrap();
            Some(transaction_id)
        } else {
            None
        }
    }

    pub fn end_transaction_at(
        &mut self,
        now: Instant,
        cx: &mut Context<Self>,
    ) -> Option<TransactionId> {
        if let Some(buffer) = self.as_singleton() {
            return buffer.update(cx, |buffer, cx| buffer.end_transaction_at(now, cx));
        }

        let mut buffer_transactions = HashMap::default();
        for BufferState { buffer, .. } in self.buffers.values() {
            if let Some(transaction_id) =
                buffer.update(cx, |buffer, cx| buffer.end_transaction_at(now, cx))
            {
                buffer_transactions.insert(buffer.read(cx).remote_id(), transaction_id);
            }
        }

        if self.history.end_transaction(now, buffer_transactions) {
            let transaction_id = self.history.group().unwrap();
            Some(transaction_id)
        } else {
            None
        }
    }

    pub fn edited_ranges_for_transaction(
        &self,
        transaction_id: TransactionId,
        cx: &App,
    ) -> Vec<Range<MultiBufferOffset>> {
        let Some(transaction) = self.history.transaction(transaction_id) else {
            return Vec::new();
        };

        let snapshot = self.read(cx);
        let mut buffer_anchors = Vec::new();

        for (buffer_id, buffer_transaction) in &transaction.buffer_transactions {
            let Some(buffer) = self.buffer(*buffer_id) else {
                continue;
            };
            let Some(excerpt) = snapshot.first_excerpt_for_buffer(*buffer_id) else {
                continue;
            };
            let buffer_snapshot = buffer.read(cx).snapshot();

            for range in buffer
                .read(cx)
                .edited_ranges_for_transaction_id::<usize>(*buffer_transaction)
            {
                buffer_anchors.push(Anchor::in_buffer(
                    excerpt.path_key_index,
                    buffer_snapshot.anchor_at(range.start, Bias::Left),
                ));
                buffer_anchors.push(Anchor::in_buffer(
                    excerpt.path_key_index,
                    buffer_snapshot.anchor_at(range.end, Bias::Right),
                ));
            }
        }
        buffer_anchors.sort_unstable_by(|a, b| a.cmp(b, &snapshot));

        snapshot
            .summaries_for_anchors(buffer_anchors.iter())
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&[s, e]| s..e)
            .collect::<Vec<_>>()
    }

    pub fn merge_transactions(
        &mut self,
        transaction: TransactionId,
        destination: TransactionId,
        cx: &mut Context<Self>,
    ) {
        if let Some(buffer) = self.as_singleton() {
            buffer.update(cx, |buffer, _| {
                buffer.merge_transactions(transaction, destination)
            });
        } else if let Some(transaction) = self.history.forget(transaction)
            && let Some(destination) = self.history.transaction_mut(destination)
        {
            for (buffer_id, buffer_transaction_id) in transaction.buffer_transactions {
                if let Some(destination_buffer_transaction_id) =
                    destination.buffer_transactions.get(&buffer_id)
                {
                    if let Some(state) = self.buffers.get(&buffer_id) {
                        state.buffer.update(cx, |buffer, _| {
                            buffer.merge_transactions(
                                buffer_transaction_id,
                                *destination_buffer_transaction_id,
                            )
                        });
                    }
                } else {
                    destination
                        .buffer_transactions
                        .insert(buffer_id, buffer_transaction_id);
                }
            }
        }
    }

    pub fn finalize_last_transaction(&mut self, cx: &mut Context<Self>) {
        self.history.finalize_last_transaction();
        for BufferState { buffer, .. } in self.buffers.values() {
            buffer.update(cx, |buffer, _| {
                buffer.finalize_last_transaction();
            });
        }
    }

    pub fn push_transaction<'a, T>(&mut self, buffer_transactions: T, cx: &Context<Self>)
    where
        T: IntoIterator<Item = (&'a Entity<Buffer>, &'a language::Transaction)>,
    {
        self.history
            .push_transaction(buffer_transactions, Instant::now(), cx);
        self.history.finalize_last_transaction();
    }

    pub fn group_until_transaction(
        &mut self,
        transaction_id: TransactionId,
        cx: &mut Context<Self>,
    ) {
        if let Some(buffer) = self.as_singleton() {
            buffer.update(cx, |buffer, _| {
                buffer.group_until_transaction(transaction_id)
            });
        } else {
            self.history.group_until(transaction_id);
        }
    }
    pub fn undo(&mut self, cx: &mut Context<Self>) -> Option<TransactionId> {
        let mut transaction_id = None;
        if let Some(buffer) = self.as_singleton() {
            transaction_id = buffer.update(cx, |buffer, cx| buffer.undo(cx));
        } else {
            while let Some(transaction) = self.history.pop_undo() {
                let mut undone = false;
                for (buffer_id, buffer_transaction_id) in &mut transaction.buffer_transactions {
                    if let Some(BufferState { buffer, .. }) = self.buffers.get(buffer_id) {
                        undone |= buffer.update(cx, |buffer, cx| {
                            let undo_to = *buffer_transaction_id;
                            if let Some(entry) = buffer.peek_undo_stack() {
                                *buffer_transaction_id = entry.transaction_id();
                            }
                            buffer.undo_to_transaction(undo_to, cx)
                        });
                    }
                }

                if undone {
                    transaction_id = Some(transaction.id);
                    break;
                }
            }
        }

        if let Some(transaction_id) = transaction_id {
            cx.emit(Event::TransactionUndone { transaction_id });
        }

        transaction_id
    }

    pub fn redo(&mut self, cx: &mut Context<Self>) -> Option<TransactionId> {
        if let Some(buffer) = self.as_singleton() {
            return buffer.update(cx, |buffer, cx| buffer.redo(cx));
        }

        while let Some(transaction) = self.history.pop_redo() {
            let mut redone = false;
            for (buffer_id, buffer_transaction_id) in transaction.buffer_transactions.iter_mut() {
                if let Some(BufferState { buffer, .. }) = self.buffers.get(buffer_id) {
                    redone |= buffer.update(cx, |buffer, cx| {
                        let redo_to = *buffer_transaction_id;
                        if let Some(entry) = buffer.peek_redo_stack() {
                            *buffer_transaction_id = entry.transaction_id();
                        }
                        buffer.redo_to_transaction(redo_to, cx)
                    });
                }
            }

            if redone {
                return Some(transaction.id);
            }
        }

        None
    }

    pub fn undo_transaction(&mut self, transaction_id: TransactionId, cx: &mut Context<Self>) {
        if let Some(buffer) = self.as_singleton() {
            buffer.update(cx, |buffer, cx| buffer.undo_transaction(transaction_id, cx));
        } else if let Some(transaction) = self.history.remove_from_undo(transaction_id) {
            for (buffer_id, transaction_id) in &transaction.buffer_transactions {
                if let Some(BufferState { buffer, .. }) = self.buffers.get(buffer_id) {
                    buffer.update(cx, |buffer, cx| {
                        buffer.undo_transaction(*transaction_id, cx)
                    });
                }
            }
        }
    }

    pub fn forget_transaction(&mut self, transaction_id: TransactionId, cx: &mut Context<Self>) {
        if let Some(buffer) = self.as_singleton() {
            buffer.update(cx, |buffer, _| {
                buffer.forget_transaction(transaction_id);
            });
        } else if let Some(transaction) = self.history.forget(transaction_id) {
            for (buffer_id, buffer_transaction_id) in transaction.buffer_transactions {
                if let Some(state) = self.buffers.get_mut(&buffer_id) {
                    state.buffer.update(cx, |buffer, _| {
                        buffer.forget_transaction(buffer_transaction_id);
                    });
                }
            }
        }
    }
}
