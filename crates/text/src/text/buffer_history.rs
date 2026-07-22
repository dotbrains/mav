use super::*;

impl Buffer {
    pub(crate) fn apply_undo(&mut self, undo: &UndoOperation) {
        self.snapshot.undo_map.insert(undo);

        let mut edits = Patch::default();
        let mut old_fragments = self
            .fragments
            .cursor::<Dimensions<Option<&Locator>, usize>>(&None);
        let mut new_fragments = SumTree::new(&None);
        let mut new_ropes =
            RopeBuilder::new(self.visible_text.cursor(0), self.deleted_text.cursor(0));

        for fragment_id in self.fragment_ids_for_edits(undo.counts.keys()) {
            let preceding_fragments = old_fragments.slice(&Some(fragment_id), Bias::Left);
            new_ropes.append(preceding_fragments.summary().text);
            new_fragments.append(preceding_fragments, &None);

            if let Some(fragment) = old_fragments.item() {
                let mut fragment = fragment.clone();
                let fragment_was_visible = fragment.visible;

                fragment.visible = fragment.is_visible(&self.undo_map);
                fragment.max_undos.observe(undo.timestamp);

                let old_start = old_fragments.start().1;
                let new_start = new_fragments.summary().text.visible;
                if fragment_was_visible && !fragment.visible {
                    edits.push(Edit {
                        old: old_start..old_start + fragment.len as usize,
                        new: new_start..new_start,
                    });
                } else if !fragment_was_visible && fragment.visible {
                    edits.push(Edit {
                        old: old_start..old_start,
                        new: new_start..new_start + fragment.len as usize,
                    });
                }
                new_ropes.push_fragment(&fragment, fragment_was_visible);
                new_fragments.push(fragment, &None);

                old_fragments.next();
            }
        }

        let suffix = old_fragments.suffix();
        new_ropes.append(suffix.summary().text);
        new_fragments.append(suffix, &None);

        drop(old_fragments);
        let (visible_text, deleted_text) = new_ropes.finish();
        self.snapshot.fragments = new_fragments;
        self.snapshot.visible_text = visible_text;
        self.snapshot.deleted_text = deleted_text;
        self.subscriptions.publish_mut(&edits);
    }

    pub(crate) fn flush_deferred_ops(&mut self) {
        self.deferred_replicas.clear();
        let mut deferred_ops = Vec::new();
        for op in self.deferred_ops.drain().iter().cloned() {
            if self.can_apply_op(&op) {
                self.apply_op(op);
            } else {
                self.deferred_replicas.insert(op.replica_id());
                deferred_ops.push(op);
            }
        }
        self.deferred_ops.insert(deferred_ops);
    }

    pub(crate) fn can_apply_op(&self, op: &Operation) -> bool {
        if self.deferred_replicas.contains(&op.replica_id()) {
            false
        } else {
            self.version.observed_all(match op {
                Operation::Edit(edit) => &edit.version,
                Operation::Undo(undo) => &undo.version,
            })
        }
    }

    pub fn has_deferred_ops(&self) -> bool {
        !self.deferred_ops.is_empty()
    }

    pub fn peek_undo_stack(&self) -> Option<&HistoryEntry> {
        self.history.undo_stack.last()
    }

    pub fn peek_redo_stack(&self) -> Option<&HistoryEntry> {
        self.history.redo_stack.last()
    }

    pub fn start_transaction(&mut self) -> Option<TransactionId> {
        self.start_transaction_at(Instant::now())
    }

    pub fn start_transaction_at(&mut self, now: Instant) -> Option<TransactionId> {
        self.history
            .start_transaction(self.version.clone(), now, &mut self.lamport_clock)
    }

    pub fn end_transaction(&mut self) -> Option<(TransactionId, clock::Global)> {
        self.end_transaction_at(Instant::now())
    }

    pub fn end_transaction_at(&mut self, now: Instant) -> Option<(TransactionId, clock::Global)> {
        if let Some(entry) = self.history.end_transaction(now) {
            let since = entry.transaction.start.clone();
            let id = self.history.group().unwrap();
            Some((id, since))
        } else {
            None
        }
    }

    pub fn finalize_last_transaction(&mut self) -> Option<&Transaction> {
        self.history.finalize_last_transaction()
    }

    pub fn group_until_transaction(&mut self, transaction_id: TransactionId) {
        self.history.group_until(transaction_id);
    }

    pub fn base_text(&self) -> &Rope {
        &self.history.base_text
    }

    pub fn operations(&self) -> &TreeMap<clock::Lamport, Operation> {
        &self.history.operations
    }

    pub fn undo(&mut self) -> Option<(TransactionId, Operation)> {
        if let Some(entry) = self.history.pop_undo() {
            let transaction = entry.transaction.clone();
            let transaction_id = transaction.id;
            let op = self.undo_or_redo(transaction);
            Some((transaction_id, op))
        } else {
            None
        }
    }

    pub fn undo_transaction(&mut self, transaction_id: TransactionId) -> Option<Operation> {
        let transaction = self
            .history
            .remove_from_undo(transaction_id)?
            .transaction
            .clone();
        Some(self.undo_or_redo(transaction))
    }

    pub fn undo_to_transaction(&mut self, transaction_id: TransactionId) -> Vec<Operation> {
        let transactions = self
            .history
            .remove_from_undo_until(transaction_id)
            .iter()
            .map(|entry| entry.transaction.clone())
            .collect::<Vec<_>>();

        transactions
            .into_iter()
            .map(|transaction| self.undo_or_redo(transaction))
            .collect()
    }

    pub fn forget_transaction(&mut self, transaction_id: TransactionId) -> Option<Transaction> {
        self.history.forget(transaction_id)
    }

    pub fn get_transaction(&self, transaction_id: TransactionId) -> Option<&Transaction> {
        self.history.transaction(transaction_id)
    }

    pub fn merge_transactions(&mut self, transaction: TransactionId, destination: TransactionId) {
        self.history.merge_transactions(transaction, destination);
    }

    pub fn redo(&mut self) -> Option<(TransactionId, Operation)> {
        if let Some(entry) = self.history.pop_redo() {
            let transaction = entry.transaction.clone();
            let transaction_id = transaction.id;
            let op = self.undo_or_redo(transaction);
            Some((transaction_id, op))
        } else {
            None
        }
    }

    pub fn redo_to_transaction(&mut self, transaction_id: TransactionId) -> Vec<Operation> {
        let transactions = self
            .history
            .remove_from_redo(transaction_id)
            .iter()
            .map(|entry| entry.transaction.clone())
            .collect::<Vec<_>>();

        transactions
            .into_iter()
            .map(|transaction| self.undo_or_redo(transaction))
            .collect()
    }

    pub(crate) fn undo_or_redo(&mut self, transaction: Transaction) -> Operation {
        let mut counts = HashMap::default();
        for edit_id in transaction.edit_ids {
            counts.insert(edit_id, self.undo_map.undo_count(edit_id).saturating_add(1));
        }

        let operation = self.undo_operations(counts);
        self.history.push(operation.clone());
        operation
    }

    pub fn undo_operations(&mut self, counts: HashMap<clock::Lamport, u32>) -> Operation {
        let timestamp = self.lamport_clock.tick();
        let version = self.version();
        self.snapshot.version.observe(timestamp);
        let undo = UndoOperation {
            timestamp,
            version,
            counts,
        };
        self.apply_undo(&undo);
        Operation::Undo(undo)
    }

    pub fn push_transaction(&mut self, transaction: Transaction, now: Instant) {
        self.history.push_transaction(transaction, now);
    }

    /// Differs from `push_transaction` in that it does not clear the redo stack.
    /// The caller responsible for
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
    pub fn push_empty_transaction(&mut self, now: Instant) -> TransactionId {
        self.history
            .push_empty_transaction(self.version.clone(), now, &mut self.lamport_clock)
    }

    pub fn edited_ranges_for_transaction_id<D>(
        &self,
        transaction_id: TransactionId,
    ) -> impl '_ + Iterator<Item = Range<D>>
    where
        D: TextDimension,
    {
        self.history
            .transaction(transaction_id)
            .into_iter()
            .flat_map(|transaction| self.edited_ranges_for_transaction(transaction))
    }

    pub fn edited_ranges_for_edit_ids<'a, D>(
        &'a self,
        edit_ids: impl IntoIterator<Item = &'a clock::Lamport>,
    ) -> impl 'a + Iterator<Item = Range<D>>
    where
        D: TextDimension,
    {
        // get fragment ranges
        let mut cursor = self
            .fragments
            .cursor::<Dimensions<Option<&Locator>, usize>>(&None);
        let offset_ranges = self
            .fragment_ids_for_edits(edit_ids.into_iter())
            .into_iter()
            .filter_map(move |fragment_id| {
                cursor.seek_forward(&Some(fragment_id), Bias::Left);
                let fragment = cursor.item()?;
                let start_offset = cursor.start().1;
                let end_offset = start_offset
                    + if fragment.visible {
                        fragment.len as usize
                    } else {
                        0
                    };
                Some(start_offset..end_offset)
            });

        // combine adjacent ranges
        let mut prev_range: Option<Range<usize>> = None;
        let disjoint_ranges = offset_ranges
            .map(Some)
            .chain([None])
            .filter_map(move |range| {
                if let Some((range, prev_range)) = range.as_ref().zip(prev_range.as_mut())
                    && prev_range.end == range.start
                {
                    prev_range.end = range.end;
                    return None;
                }
                let result = prev_range.clone();
                prev_range = range;
                result
            });

        // convert to the desired text dimension.
        let mut position = D::zero(());
        let mut rope_cursor = self.visible_text.cursor(0);
        disjoint_ranges.map(move |range| {
            position.add_assign(&rope_cursor.summary(range.start));
            let start = position;
            position.add_assign(&rope_cursor.summary(range.end));
            let end = position;
            start..end
        })
    }

    pub fn edited_ranges_for_transaction<'a, D>(
        &'a self,
        transaction: &'a Transaction,
    ) -> impl 'a + Iterator<Item = Range<D>>
    where
        D: TextDimension,
    {
        self.edited_ranges_for_edit_ids(&transaction.edit_ids)
    }

    pub fn subscribe(&mut self) -> Subscription<usize> {
        self.subscriptions.subscribe()
    }

    pub fn wait_for_edits<It: IntoIterator<Item = clock::Lamport>>(
        &mut self,
        edit_ids: It,
    ) -> impl 'static + Future<Output = Result<()>> + use<It> {
        let mut futures = Vec::new();
        for edit_id in edit_ids {
            if !self.version.observed(edit_id) {
                let (tx, rx) = oneshot::channel();
                self.edit_id_resolvers.entry(edit_id).or_default().push(tx);
                futures.push(rx);
            }
        }

        async move {
            for mut future in futures {
                if future.recv().await.is_none() {
                    anyhow::bail!("gave up waiting for edits");
                }
            }
            Ok(())
        }
    }

    pub fn wait_for_anchors<It: IntoIterator<Item = Anchor>>(
        &mut self,
        anchors: It,
    ) -> impl 'static + Future<Output = Result<()>> + use<It> {
        let mut futures = Vec::new();
        for anchor in anchors {
            if !self.version.observed(anchor.timestamp()) && !anchor.is_max() && !anchor.is_min() {
                let (tx, rx) = oneshot::channel();
                self.edit_id_resolvers
                    .entry(anchor.timestamp())
                    .or_default()
                    .push(tx);
                futures.push(rx);
            }
        }

        async move {
            for mut future in futures {
                if future.recv().await.is_none() {
                    anyhow::bail!("gave up waiting for anchors");
                }
            }
            Ok(())
        }
    }

    pub fn wait_for_version(
        &mut self,
        version: clock::Global,
    ) -> impl Future<Output = Result<()>> + use<> {
        let mut rx = None;
        if !self.snapshot.version.observed_all(&version) {
            let channel = oneshot::channel();
            self.wait_for_version_txs.push((version, channel.0));
            rx = Some(channel.1);
        }
        async move {
            if let Some(mut rx) = rx
                && rx.recv().await.is_none()
            {
                anyhow::bail!("gave up waiting for version");
            }
            Ok(())
        }
    }

    pub fn give_up_waiting(&mut self) {
        self.edit_id_resolvers.clear();
        self.wait_for_version_txs.clear();
    }

    pub(crate) fn resolve_edit(&mut self, edit_id: clock::Lamport) {
        for mut tx in self
            .edit_id_resolvers
            .remove(&edit_id)
            .into_iter()
            .flatten()
        {
            tx.try_send(()).ok();
        }
    }

    pub fn set_group_interval(&mut self, group_interval: Duration) {
        self.history.group_interval = group_interval;
    }

    pub fn snapshot_with_edits<I, S, T>(&mut self, edits: I) -> EditedBufferSnapshot
    where
        I: IntoIterator<Item = (Range<S>, T)>,
        S: ToOffset,
        T: Into<Arc<str>>,
    {
        let mut snapshot = self.snapshot.clone();
        let base_version = self.version();
        let edits: Vec<_> = edits
            .into_iter()
            .map(|(range, new_text)| (range.to_offset(&snapshot), new_text.into()))
            .collect();
        if edits.is_empty() {
            return EditedBufferSnapshot {
                base_version,
                snapshot,
                did_edit: false,
            };
        }
        let timestamp = self.lamport_clock.tick();
        snapshot.apply_edit_internal(edits, timestamp);
        snapshot.version.observe(timestamp);
        EditedBufferSnapshot {
            base_version,
            snapshot,
            did_edit: true,
        }
    }

    pub fn fast_forward(&mut self, edited: EditedBufferSnapshot) {
        if self.version.changed_since(&edited.base_version) {
            panic!("buffer cannot be fast-forwarded")
        }
        self.snapshot = edited.snapshot.clone();
        for timestamp in edited.snapshot.version.iter() {
            self.lamport_clock.observe(timestamp);
        }
    }
}
