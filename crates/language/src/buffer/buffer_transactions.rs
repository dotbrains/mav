use super::*;

impl Buffer {
    pub fn start_transaction(&mut self) -> Option<TransactionId> {
        self.start_transaction_at(Instant::now())
    }

    /// Starts a transaction, providing the current time. Subsequent transactions
    /// that occur within a short period of time will be grouped together. This
    /// is controlled by the buffer's undo grouping duration.
    pub fn start_transaction_at(&mut self, now: Instant) -> Option<TransactionId> {
        self.transaction_depth += 1;
        if self.was_dirty_before_starting_transaction.is_none() {
            self.was_dirty_before_starting_transaction = Some(self.is_dirty());
        }
        self.text.start_transaction_at(now)
    }

    /// Terminates the current transaction, if this is the outermost transaction.
    pub fn end_transaction(&mut self, cx: &mut Context<Self>) -> Option<TransactionId> {
        self.end_transaction_at(Instant::now(), cx)
    }

    pub fn end_transaction_with_source(
        &mut self,
        source: BufferEditSource,
        cx: &mut Context<Self>,
    ) -> Option<TransactionId> {
        self.end_transaction_at_internal(Instant::now(), source, cx)
    }

    /// Terminates the current transaction, providing the current time. Subsequent transactions
    /// that occur within a short period of time will be grouped together. This
    /// is controlled by the buffer's undo grouping duration.
    pub fn end_transaction_at(
        &mut self,
        now: Instant,
        cx: &mut Context<Self>,
    ) -> Option<TransactionId> {
        self.end_transaction_at_internal(now, BufferEditSource::User, cx)
    }

    fn end_transaction_at_internal(
        &mut self,
        now: Instant,
        source: BufferEditSource,
        cx: &mut Context<Self>,
    ) -> Option<TransactionId> {
        assert!(self.transaction_depth > 0);
        self.transaction_depth -= 1;
        let was_dirty = if self.transaction_depth == 0 {
            self.was_dirty_before_starting_transaction.take().unwrap()
        } else {
            false
        };
        if let Some((transaction_id, start_version)) = self.text.end_transaction_at(now) {
            self.did_edit(&start_version, was_dirty, source, cx);
            Some(transaction_id)
        } else {
            None
        }
    }

    /// Manually add a transaction to the buffer's undo history.
    pub fn push_transaction(&mut self, transaction: Transaction, now: Instant) {
        self.text.push_transaction(transaction, now);
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
    pub fn push_empty_transaction(&mut self, now: Instant) -> TransactionId {
        self.text.push_empty_transaction(now)
    }

    /// Prevent the last transaction from being grouped with any subsequent transactions,
    /// even if they occur with the buffer's undo grouping duration.
    pub fn finalize_last_transaction(&mut self) -> Option<&Transaction> {
        self.text.finalize_last_transaction()
    }

    /// Manually group all changes since a given transaction.
    pub fn group_until_transaction(&mut self, transaction_id: TransactionId) {
        self.text.group_until_transaction(transaction_id);
    }

    /// Manually remove a transaction from the buffer's undo history
    pub fn forget_transaction(&mut self, transaction_id: TransactionId) -> Option<Transaction> {
        self.text.forget_transaction(transaction_id)
    }

    /// Retrieve a transaction from the buffer's undo history
    pub fn get_transaction(&self, transaction_id: TransactionId) -> Option<&Transaction> {
        self.text.get_transaction(transaction_id)
    }

    /// Manually merge two transactions in the buffer's undo history.
    pub fn merge_transactions(&mut self, transaction: TransactionId, destination: TransactionId) {
        self.text.merge_transactions(transaction, destination);
    }

    /// Waits for the buffer to receive operations with the given timestamps.
    pub fn wait_for_edits<It: IntoIterator<Item = clock::Lamport>>(
        &mut self,
        edit_ids: It,
    ) -> impl Future<Output = Result<()>> + use<It> {
        self.text.wait_for_edits(edit_ids)
    }

    /// Waits for the buffer to receive the operations necessary for resolving the given anchors.
    pub fn wait_for_anchors<It: IntoIterator<Item = Anchor>>(
        &mut self,
        anchors: It,
    ) -> impl 'static + Future<Output = Result<()>> + use<It> {
        self.text.wait_for_anchors(anchors)
    }

    /// Waits for the buffer to receive operations up to the given version.
    pub fn wait_for_version(
        &mut self,
        version: clock::Global,
    ) -> impl Future<Output = Result<()>> + use<> {
        self.text.wait_for_version(version)
    }

    /// Forces all futures returned by [`Buffer::wait_for_version`], [`Buffer::wait_for_edits`], or
    /// [`Buffer::wait_for_version`] to resolve with an error.
    pub fn give_up_waiting(&mut self) {
        self.text.give_up_waiting();
    }

    pub fn wait_for_autoindent_applied(&mut self) -> Option<oneshot::Receiver<()>> {
        let mut rx = None;
        if !self.autoindent_requests.is_empty() {
            let channel = oneshot::channel();
            self.wait_for_autoindent_txs.push(channel.0);
            rx = Some(channel.1);
        }
        rx
    }
}
