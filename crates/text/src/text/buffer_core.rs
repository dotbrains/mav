use super::*;

impl Buffer {
    pub fn new(replica_id: ReplicaId, remote_id: BufferId, base_text: impl Into<String>) -> Buffer {
        let mut base_text = base_text.into();
        let line_ending = LineEnding::detect(&base_text);
        LineEnding::normalize(&mut base_text);
        Self::new_normalized(replica_id, remote_id, line_ending, Rope::from(&*base_text))
    }

    pub fn new_normalized(
        replica_id: ReplicaId,
        remote_id: BufferId,
        line_ending: LineEnding,
        normalized: Rope,
    ) -> Buffer {
        let history = History::new(normalized);
        let mut fragments = SumTree::new(&None);
        let mut insertions = SumTree::default();

        let mut lamport_clock = clock::Lamport::new(replica_id);
        let mut version = clock::Global::new();

        let visible_text = history.base_text.clone();
        if !visible_text.is_empty() {
            let insertion_timestamp = clock::Lamport::new(ReplicaId::LOCAL);
            lamport_clock.observe(insertion_timestamp);
            version.observe(insertion_timestamp);

            let mut insertion_offset: u32 = 0;
            let mut text_offset: usize = 0;
            let mut prev_locator = Locator::min();

            while text_offset < visible_text.len() {
                let target_end = visible_text.len().min(text_offset + MAX_INSERTION_LEN);
                let chunk_end = if target_end == visible_text.len() {
                    target_end
                } else {
                    visible_text.floor_char_boundary(target_end)
                };
                let chunk_len = chunk_end - text_offset;

                let fragment_id = Locator::between(&prev_locator, &Locator::max());
                let fragment = Fragment {
                    id: fragment_id.clone(),
                    timestamp: insertion_timestamp,
                    insertion_offset,
                    len: chunk_len as u32,
                    visible: true,
                    deletions: Default::default(),
                    max_undos: Default::default(),
                };
                insertions.push(InsertionFragment::new(&fragment), ());
                fragments.push(fragment, &None);

                prev_locator = fragment_id;
                insertion_offset += chunk_len as u32;
                text_offset = chunk_end;
            }
        }

        Buffer {
            snapshot: BufferSnapshot {
                replica_id,
                remote_id,
                visible_text,
                deleted_text: Rope::new(),
                line_ending,
                fragments,
                insertions,
                version,
                undo_map: Default::default(),
                insertion_slices: Default::default(),
            },
            history,
            deferred_ops: OperationQueue::new(),
            deferred_replicas: HashSet::default(),
            lamport_clock,
            subscriptions: Default::default(),
            edit_id_resolvers: Default::default(),
            wait_for_version_txs: Default::default(),
        }
    }

    pub fn version(&self) -> clock::Global {
        self.version.clone()
    }

    pub fn snapshot(&self) -> &BufferSnapshot {
        &self.snapshot
    }

    pub fn into_snapshot(self) -> BufferSnapshot {
        self.snapshot
    }

    pub fn branch(&self) -> Self {
        Self {
            snapshot: self.snapshot.clone(),
            history: History::new(self.base_text().clone()),
            deferred_ops: OperationQueue::new(),
            deferred_replicas: HashSet::default(),
            lamport_clock: clock::Lamport::new(ReplicaId::LOCAL_BRANCH),
            subscriptions: Default::default(),
            edit_id_resolvers: Default::default(),
            wait_for_version_txs: Default::default(),
        }
    }

    pub fn replica_id(&self) -> ReplicaId {
        self.lamport_clock.replica_id
    }

    pub fn remote_id(&self) -> BufferId {
        self.remote_id
    }

    pub fn deferred_ops_len(&self) -> usize {
        self.deferred_ops.len()
    }

    pub fn transaction_group_interval(&self) -> Duration {
        self.history.group_interval
    }

    pub fn edit<R, I, S, T>(&mut self, edits: R) -> Operation
    where
        R: IntoIterator<IntoIter = I>,
        I: ExactSizeIterator<Item = (Range<S>, T)>,
        S: ToOffset,
        T: Into<Arc<str>>,
    {
        let edits = edits
            .into_iter()
            .map(|(range, new_text)| (range, new_text.into()));

        self.start_transaction();
        let timestamp = self.lamport_clock.tick();
        let operation = Operation::Edit(self.apply_local_edit(edits, timestamp));

        self.history.push(operation.clone());
        self.history.push_undo(operation.timestamp());
        self.snapshot.version.observe(operation.timestamp());
        self.end_transaction();
        operation
    }

    fn apply_local_edit<S: ToOffset, T: Into<Arc<str>>>(
        &mut self,
        edits: impl ExactSizeIterator<Item = (Range<S>, T)>,
        timestamp: clock::Lamport,
    ) -> EditOperation {
        let edits: Vec<_> = edits
            .map(|(range, new_text)| (range.to_offset(&*self), new_text.into()))
            .collect();
        let (edit_op, edits_patch) = self.snapshot.apply_edit_internal(edits, timestamp);
        self.subscriptions.publish_mut(&edits_patch);
        edit_op
    }

    pub fn set_line_ending(&mut self, line_ending: LineEnding) {
        self.snapshot.line_ending = line_ending;
    }

    pub fn apply_ops<I: IntoIterator<Item = Operation>>(&mut self, ops: I) {
        let mut deferred_ops = Vec::new();
        for op in ops {
            self.history.push(op.clone());
            if self.can_apply_op(&op) {
                self.apply_op(op);
            } else {
                self.deferred_replicas.insert(op.replica_id());
                deferred_ops.push(op);
            }
        }
        self.deferred_ops.insert(deferred_ops);
        self.flush_deferred_ops();
    }

    pub(crate) fn apply_op(&mut self, op: Operation) {
        match op {
            Operation::Edit(edit) => {
                if !self.version.observed(edit.timestamp) {
                    self.apply_remote_edit(
                        &edit.version,
                        &edit.ranges,
                        &edit.new_text,
                        edit.timestamp,
                    );
                    self.snapshot.version.observe(edit.timestamp);
                    self.lamport_clock.observe(edit.timestamp);
                    self.resolve_edit(edit.timestamp);
                }
            }
            Operation::Undo(undo) => {
                if !self.version.observed(undo.timestamp) {
                    self.apply_undo(&undo);
                    self.snapshot.version.observe(undo.timestamp);
                    self.lamport_clock.observe(undo.timestamp);
                }
            }
        }
        self.wait_for_version_txs.retain_mut(|(version, tx)| {
            if self.snapshot.version().observed_all(version) {
                tx.try_send(()).ok();
                false
            } else {
                true
            }
        });
    }
}
