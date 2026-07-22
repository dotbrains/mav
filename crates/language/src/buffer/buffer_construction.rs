use super::*;

impl Buffer {
    /// Create a new buffer with the given base text.
    pub fn local<T: Into<String>>(base_text: T, cx: &Context<Self>) -> Self {
        Self::build(
            TextBuffer::new(
                ReplicaId::LOCAL,
                cx.entity_id().as_non_zero_u64().into(),
                base_text.into(),
            ),
            None,
            Capability::ReadWrite,
        )
    }

    /// Create a new buffer with the given base text that has proper line endings and other normalization applied.
    pub fn local_normalized(
        base_text_normalized: Rope,
        line_ending: LineEnding,
        cx: &Context<Self>,
    ) -> Self {
        Self::build(
            TextBuffer::new_normalized(
                ReplicaId::LOCAL,
                cx.entity_id().as_non_zero_u64().into(),
                line_ending,
                base_text_normalized,
            ),
            None,
            Capability::ReadWrite,
        )
    }

    /// Create a new buffer that is a replica of a remote buffer.
    pub fn remote(
        remote_id: BufferId,
        replica_id: ReplicaId,
        capability: Capability,
        base_text: impl Into<String>,
    ) -> Self {
        Self::build(
            TextBuffer::new(replica_id, remote_id, base_text.into()),
            None,
            capability,
        )
    }

    /// Create a new buffer that is a replica of a remote buffer, populating its
    /// state from the given protobuf message.
    pub fn from_proto(
        replica_id: ReplicaId,
        capability: Capability,
        message: proto::BufferState,
        file: Option<Arc<dyn File>>,
    ) -> Result<Self> {
        let buffer_id = BufferId::new(message.id).context("Could not deserialize buffer_id")?;
        let buffer = TextBuffer::new(replica_id, buffer_id, message.base_text);
        let mut this = Self::build(buffer, file, capability);
        this.text.set_line_ending(proto::deserialize_line_ending(
            rpc::proto::LineEnding::from_i32(message.line_ending).context("missing line_ending")?,
        ));
        this.saved_version = proto::deserialize_version(&message.saved_version);
        this.saved_mtime = message.saved_mtime.map(|time| time.into());
        Ok(this)
    }

    /// Serialize the buffer's state to a protobuf message.
    pub fn to_proto(&self, cx: &App) -> proto::BufferState {
        proto::BufferState {
            id: self.remote_id().into(),
            file: self.file.as_ref().map(|f| f.to_proto(cx)),
            base_text: self.base_text().to_string(),
            line_ending: proto::serialize_line_ending(self.line_ending()) as i32,
            saved_version: proto::serialize_version(&self.saved_version),
            saved_mtime: self.saved_mtime.map(|time| time.into()),
        }
    }

    /// Serialize as protobufs all of the changes to the buffer since the given version.
    pub fn serialize_ops(
        &self,
        since: Option<clock::Global>,
        cx: &App,
    ) -> Task<Vec<proto::Operation>> {
        let mut operations = Vec::new();
        operations.extend(self.deferred_ops.iter().map(proto::serialize_operation));

        operations.extend(self.remote_selections.iter().map(|(_, set)| {
            proto::serialize_operation(&Operation::UpdateSelections {
                selections: set.selections.clone(),
                lamport_timestamp: set.lamport_timestamp,
                line_mode: set.line_mode,
                cursor_shape: set.cursor_shape,
            })
        }));

        for (server_id, diagnostics) in self.diagnostics.iter() {
            operations.push(proto::serialize_operation(&Operation::UpdateDiagnostics {
                lamport_timestamp: self.diagnostics_timestamp,
                server_id: *server_id,
                diagnostics: diagnostics.iter().cloned().collect(),
            }));
        }

        for (server_id, completions) in &self.completion_triggers_per_language_server {
            operations.push(proto::serialize_operation(
                &Operation::UpdateCompletionTriggers {
                    triggers: completions.iter().cloned().collect(),
                    lamport_timestamp: self.completion_triggers_timestamp,
                    server_id: *server_id,
                },
            ));
        }

        let text_operations = self.text.operations().clone();
        cx.background_spawn(async move {
            let since = since.unwrap_or_default();
            operations.extend(
                text_operations
                    .iter()
                    .filter(|(_, op)| !since.observed(op.timestamp()))
                    .map(|(_, op)| proto::serialize_operation(&Operation::Buffer(op.clone()))),
            );
            operations.sort_unstable_by_key(proto::lamport_timestamp_for_operation);
            operations
        })
    }

    /// Assign a language to the buffer, returning the buffer.
    pub fn with_language_async(mut self, language: Arc<Language>, cx: &mut Context<Self>) -> Self {
        self.set_language_async(Some(language), cx);
        self
    }

    /// Assign a language to the buffer, blocking for up to 1ms to reparse the buffer, returning the buffer.
    #[ztracing::instrument(skip_all, fields(lang = language.config.name.0.as_str()))]
    pub fn with_language(mut self, language: Arc<Language>, cx: &mut Context<Self>) -> Self {
        self.set_language(Some(language), cx);
        self
    }

    /// Returns the [`Capability`] of this buffer.
    pub fn capability(&self) -> Capability {
        self.capability
    }

    /// Whether this buffer can only be read.
    pub fn read_only(&self) -> bool {
        !self.capability.editable()
    }

    /// Builds a [`Buffer`] with the given underlying [`TextBuffer`], diff base, [`File`] and [`Capability`].
    pub fn build(buffer: TextBuffer, file: Option<Arc<dyn File>>, capability: Capability) -> Self {
        let saved_mtime = file.as_ref().and_then(|file| file.disk_state().mtime());
        let snapshot = buffer.snapshot();
        let syntax_map = Mutex::new(SyntaxMap::new(&snapshot));
        let tree_sitter_data = TreeSitterData::new(snapshot);
        Self {
            saved_mtime,
            tree_sitter_data: Arc::new(tree_sitter_data),
            saved_version: buffer.version(),
            preview_version: buffer.version(),
            reload_task: None,
            transaction_depth: 0,
            was_dirty_before_starting_transaction: None,
            has_unsaved_edits: Cell::new((buffer.version(), false)),
            text: buffer,
            branch_state: None,
            file,
            capability,
            syntax_map,
            reparse: None,
            non_text_state_update_count: 0,
            sync_parse_timeout: if cfg!(any(test, feature = "test-support")) {
                Some(Duration::from_millis(10))
            } else {
                Some(Duration::from_millis(1))
            },
            parse_status: watch::channel(ParseStatus::Idle),
            autoindent_requests: Default::default(),
            wait_for_autoindent_txs: Default::default(),
            pending_autoindent: Default::default(),
            language: None,
            remote_selections: Default::default(),
            diagnostics: Default::default(),
            diagnostics_timestamp: Lamport::MIN,
            completion_triggers: Default::default(),
            completion_triggers_per_language_server: Default::default(),
            completion_triggers_timestamp: Lamport::MIN,
            deferred_ops: OperationQueue::new(),
            has_conflict: false,
            change_bits: Default::default(),
            modeline: None,
            _subscriptions: Vec::new(),
            encoding: encoding_rs::UTF_8,
            has_bom: false,
            reload_with_encoding_txns: HashMap::default(),
        }
    }

    #[ztracing::instrument(skip_all)]
    pub fn build_snapshot(
        text: Rope,
        language: Option<Arc<Language>>,
        language_registry: Option<Arc<LanguageRegistry>>,
        modeline: Option<Arc<ModelineSettings>>,
        cx: &mut App,
    ) -> impl Future<Output = BufferSnapshot> + use<> {
        let entity_id = cx.reserve_entity::<Self>().entity_id();
        let buffer_id = entity_id.as_non_zero_u64().into();
        async move {
            let text =
                TextBuffer::new_normalized(ReplicaId::LOCAL, buffer_id, Default::default(), text);
            let text = text.into_snapshot();
            let mut syntax = SyntaxMap::new(&text).snapshot();
            if let Some(language) = language.clone() {
                let language_registry = language_registry.clone();
                syntax.reparse(&text, language_registry, language);
            }
            let tree_sitter_data = TreeSitterData::new(&text);
            BufferSnapshot {
                text,
                syntax,
                file: None,
                diagnostics: Default::default(),
                remote_selections: Default::default(),
                tree_sitter_data: Arc::new(tree_sitter_data),
                language,
                non_text_state_update_count: 0,
                capability: Capability::ReadOnly,
                modeline,
            }
        }
    }

    pub fn build_empty_snapshot(cx: &mut App) -> BufferSnapshot {
        let entity_id = cx.reserve_entity::<Self>().entity_id();
        let buffer_id = entity_id.as_non_zero_u64().into();
        let text = TextBuffer::new_normalized(
            ReplicaId::LOCAL,
            buffer_id,
            Default::default(),
            Rope::new(),
        );
        let text = text.into_snapshot();
        let syntax = SyntaxMap::new(&text).snapshot();
        let tree_sitter_data = TreeSitterData::new(&text);
        BufferSnapshot {
            text,
            syntax,
            tree_sitter_data: Arc::new(tree_sitter_data),
            file: None,
            diagnostics: Default::default(),
            remote_selections: Default::default(),
            language: None,
            non_text_state_update_count: 0,
            capability: Capability::ReadOnly,
            modeline: None,
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn build_snapshot_sync(
        text: Rope,
        language: Option<Arc<Language>>,
        language_registry: Option<Arc<LanguageRegistry>>,
        cx: &mut App,
    ) -> BufferSnapshot {
        let entity_id = cx.reserve_entity::<Self>().entity_id();
        let buffer_id = entity_id.as_non_zero_u64().into();
        let text =
            TextBuffer::new_normalized(ReplicaId::LOCAL, buffer_id, Default::default(), text)
                .into_snapshot();
        let mut syntax = SyntaxMap::new(&text).snapshot();
        if let Some(language) = language.clone() {
            syntax.reparse(&text, language_registry, language);
        }
        let tree_sitter_data = TreeSitterData::new(&text);
        BufferSnapshot {
            text,
            syntax,
            tree_sitter_data: Arc::new(tree_sitter_data),
            file: None,
            diagnostics: Default::default(),
            remote_selections: Default::default(),
            language,
            non_text_state_update_count: 0,
            capability: Capability::ReadOnly,
            modeline: None,
        }
    }

    /// Retrieve a snapshot of the buffer's current state. This is computationally
    /// cheap, and allows reading from the buffer on a background thread.
    pub fn snapshot(&self) -> BufferSnapshot {
        let text = self.text.snapshot();

        let syntax = {
            let mut syntax_map = self.syntax_map.lock();
            syntax_map.interpolate(text);
            syntax_map.snapshot()
        };

        let tree_sitter_data = if self.text.version() != *self.tree_sitter_data.version() {
            Arc::new(TreeSitterData::new(text))
        } else {
            self.tree_sitter_data.clone()
        };

        BufferSnapshot {
            text: text.clone(),
            syntax,
            tree_sitter_data,
            file: self.file.clone(),
            remote_selections: self.remote_selections.clone(),
            diagnostics: self.diagnostics.clone(),
            language: self.language.clone(),
            non_text_state_update_count: self.non_text_state_update_count,
            capability: self.capability,
            modeline: self.modeline.clone(),
        }
    }

    pub fn branch(&mut self, cx: &mut Context<Self>) -> Entity<Self> {
        let this = cx.entity();
        cx.new(|cx| {
            let mut branch = Self {
                branch_state: Some(BufferBranchState {
                    base_buffer: this.clone(),
                    merged_operations: Default::default(),
                }),
                language: self.language.clone(),
                has_conflict: self.has_conflict,
                has_unsaved_edits: Cell::new(self.has_unsaved_edits.get_mut().clone()),
                _subscriptions: vec![cx.subscribe(&this, Self::on_base_buffer_event)],
                ..Self::build(self.text.branch(), self.file.clone(), self.capability())
            };
            if let Some(language_registry) = self.language_registry() {
                branch.set_language_registry(language_registry);
            }

            // Reparse the branch buffer so that we get syntax highlighting immediately.
            branch.reparse(cx, true);

            branch
        })
    }
}
