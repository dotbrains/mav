use super::*;

impl Buffer {
    pub fn as_text_snapshot(&self) -> &text::BufferSnapshot {
        &self.text
    }

    /// Retrieve a snapshot of the buffer's raw text, without any
    /// language-related state like the syntax tree or diagnostics.
    #[ztracing::instrument(skip_all)]
    pub fn text_snapshot(&self) -> text::BufferSnapshot {
        // todo lw
        self.text.snapshot().clone()
    }

    /// The file associated with the buffer, if any.
    pub fn file(&self) -> Option<&Arc<dyn File>> {
        self.file.as_ref()
    }

    /// The version of the buffer that was last saved or reloaded from disk.
    pub fn saved_version(&self) -> &clock::Global {
        &self.saved_version
    }

    /// The mtime of the buffer's file when the buffer was last saved or reloaded from disk.
    pub fn saved_mtime(&self) -> Option<MTime> {
        self.saved_mtime
    }

    /// Returns the character encoding of the buffer's file.
    pub fn encoding(&self) -> &'static Encoding {
        self.encoding
    }

    /// Sets the character encoding of the buffer.
    pub fn set_encoding(&mut self, encoding: &'static Encoding) {
        self.encoding = encoding;
    }

    /// Returns whether the buffer has a Byte Order Mark.
    pub fn has_bom(&self) -> bool {
        self.has_bom
    }

    /// Sets whether the buffer has a Byte Order Mark.
    pub fn set_has_bom(&mut self, has_bom: bool) {
        self.has_bom = has_bom;
    }

    /// Assign a language to the buffer.
    pub fn set_language_async(&mut self, language: Option<Arc<Language>>, cx: &mut Context<Self>) {
        self.set_language_(language, cfg!(any(test, feature = "test-support")), cx);
    }

    /// Assign a language to the buffer, blocking for up to 1ms to reparse the buffer.
    pub fn set_language(&mut self, language: Option<Arc<Language>>, cx: &mut Context<Self>) {
        self.set_language_(language, true, cx);
    }

    #[ztracing::instrument(skip_all)]
    fn set_language_(
        &mut self,
        language: Option<Arc<Language>>,
        may_block: bool,
        cx: &mut Context<Self>,
    ) {
        if language == self.language {
            return;
        }
        self.non_text_state_update_count += 1;
        self.syntax_map.lock().clear(&self.text);
        let old_language = std::mem::replace(&mut self.language, language);
        self.was_changed();
        self.reparse(cx, may_block);
        let has_fresh_language =
            self.language.is_some() && old_language.is_none_or(|old| old == *PLAIN_TEXT);
        cx.emit(BufferEvent::LanguageChanged(has_fresh_language));
    }

    /// Assign a language registry to the buffer. This allows the buffer to retrieve
    /// other languages if parts of the buffer are written in different languages.
    pub fn set_language_registry(&self, language_registry: Arc<LanguageRegistry>) {
        self.syntax_map
            .lock()
            .set_language_registry(language_registry);
    }

    pub fn language_registry(&self) -> Option<Arc<LanguageRegistry>> {
        self.syntax_map.lock().language_registry()
    }

    /// Assign the line ending type to the buffer.
    pub fn set_line_ending(&mut self, line_ending: LineEnding, cx: &mut Context<Self>) {
        self.text.set_line_ending(line_ending);

        let lamport_timestamp = self.text.lamport_clock.tick();
        self.send_operation(
            Operation::UpdateLineEnding {
                line_ending,
                lamport_timestamp,
            },
            true,
            cx,
        );
    }

    /// Assign the buffer [`ModelineSettings`].
    pub fn set_modeline(&mut self, modeline: Option<ModelineSettings>) -> bool {
        if modeline.as_ref() != self.modeline.as_deref() {
            self.modeline = modeline.map(Arc::new);
            true
        } else {
            false
        }
    }

    /// Returns the [`ModelineSettings`].
    pub fn modeline(&self) -> Option<&Arc<ModelineSettings>> {
        self.modeline.as_ref()
    }

    /// Assign the buffer a new [`Capability`].
    pub fn set_capability(&mut self, capability: Capability, cx: &mut Context<Self>) {
        if self.capability != capability {
            self.capability = capability;
            cx.emit(BufferEvent::CapabilityChanged)
        }
    }

    /// This method is called to signal that the buffer has been saved.
    pub fn did_save(
        &mut self,
        version: clock::Global,
        mtime: Option<MTime>,
        cx: &mut Context<Self>,
    ) {
        self.saved_version = version.clone();
        self.has_unsaved_edits.set((version, false));
        self.has_conflict = false;
        self.saved_mtime = mtime;
        self.was_changed();
        cx.emit(BufferEvent::Saved);
        cx.notify();
    }

    /// Reloads the contents of the buffer from disk.
    pub fn reload(&mut self, cx: &Context<Self>) -> oneshot::Receiver<Option<Transaction>> {
        self.reload_impl(None, cx)
    }

    /// Reloads the contents of the buffer from disk using the specified encoding.
    ///
    /// This bypasses automatic encoding detection heuristics (like BOM checks) for non-Unicode encodings,
    /// allowing users to force a specific interpretation of the bytes.
    pub fn reload_with_encoding(
        &mut self,
        encoding: &'static Encoding,
        cx: &Context<Self>,
    ) -> oneshot::Receiver<Option<Transaction>> {
        self.reload_impl(Some(encoding), cx)
    }

    fn reload_impl(
        &mut self,
        force_encoding: Option<&'static Encoding>,
        cx: &Context<Self>,
    ) -> oneshot::Receiver<Option<Transaction>> {
        let (tx, rx) = futures::channel::oneshot::channel();
        let prev_version = self.text.version();

        self.reload_task = Some(cx.spawn(async move |this, cx| {
            let Some((new_mtime, load_bytes_task, current_encoding)) =
                this.update(cx, |this, cx| {
                    let file = this.file.as_ref()?.as_local()?;
                    Some((
                        file.disk_state().mtime(),
                        file.load_bytes(cx),
                        this.encoding,
                    ))
                })?
            else {
                return Ok(());
            };

            let target_encoding = force_encoding.unwrap_or(current_encoding);

            let bytes = load_bytes_task.await?;

            anyhow::ensure!(
                analyze_byte_content(&bytes) != ByteContent::Binary,
                "Binary files are not supported"
            );

            let is_unicode = target_encoding == encoding_rs::UTF_8
                || target_encoding == encoding_rs::UTF_16LE
                || target_encoding == encoding_rs::UTF_16BE;

            let (new_text, has_bom, encoding_used) = if force_encoding.is_some() && !is_unicode {
                let (cow, _had_errors) = target_encoding.decode_without_bom_handling(&bytes);
                (cow.into_owned(), false, target_encoding)
            } else {
                let (cow, used_enc, _had_errors) = target_encoding.decode(&bytes);

                let actual_has_bom = if used_enc == encoding_rs::UTF_8 {
                    bytes.starts_with(&[0xEF, 0xBB, 0xBF])
                } else if used_enc == encoding_rs::UTF_16LE {
                    bytes.starts_with(&[0xFF, 0xFE])
                } else if used_enc == encoding_rs::UTF_16BE {
                    bytes.starts_with(&[0xFE, 0xFF])
                } else {
                    false
                };
                (cow.into_owned(), actual_has_bom, used_enc)
            };

            let diff = this.update(cx, |this, cx| this.diff(new_text, cx))?.await;
            this.update(cx, |this, cx| {
                if this.version() == diff.base_version {
                    this.finalize_last_transaction();
                    let old_encoding = this.encoding;
                    let old_has_bom = this.has_bom;
                    this.apply_diff(diff, cx);
                    this.encoding = encoding_used;
                    this.has_bom = has_bom;
                    let transaction = this.finalize_last_transaction().cloned();
                    if let Some(ref txn) = transaction {
                        if old_encoding != encoding_used || old_has_bom != has_bom {
                            this.reload_with_encoding_txns
                                .insert(txn.id, (old_encoding, old_has_bom));
                        }
                    }
                    tx.send(transaction).ok();
                    this.has_conflict = false;
                    this.did_reload(this.version(), this.line_ending(), new_mtime, cx);
                } else {
                    if !diff.edits.is_empty()
                        || this
                            .edits_since::<usize>(&diff.base_version)
                            .next()
                            .is_some()
                    {
                        this.has_conflict = true;
                    }

                    this.did_reload(prev_version, this.line_ending(), this.saved_mtime, cx);
                }

                this.reload_task.take();
            })
        }));
        rx
    }

    /// This method is called to signal that the buffer has been reloaded.
    pub fn did_reload(
        &mut self,
        version: clock::Global,
        line_ending: LineEnding,
        mtime: Option<MTime>,
        cx: &mut Context<Self>,
    ) {
        self.saved_version = version;
        self.has_unsaved_edits
            .set((self.saved_version.clone(), false));
        self.text.set_line_ending(line_ending);
        self.saved_mtime = mtime;
        cx.emit(BufferEvent::Reloaded);
        cx.notify();
    }

    /// Updates the [`File`] backing this buffer. This should be called when
    /// the file has changed or has been deleted.
    pub fn file_updated(&mut self, new_file: Arc<dyn File>, cx: &mut Context<Self>) {
        let was_dirty = self.is_dirty();
        let mut file_changed = false;

        if let Some(old_file) = self.file.as_ref() {
            if new_file.path() != old_file.path() {
                file_changed = true;
            }

            let old_state = old_file.disk_state();
            let new_state = new_file.disk_state();
            if old_state != new_state {
                file_changed = true;
                if !was_dirty && matches!(new_state, DiskState::Present { .. }) {
                    cx.emit(BufferEvent::ReloadNeeded)
                }
            }
        } else {
            file_changed = true;
        };

        self.file = Some(new_file);
        if file_changed {
            self.was_changed();
            self.non_text_state_update_count += 1;
            if was_dirty != self.is_dirty() {
                cx.emit(BufferEvent::DirtyChanged);
            }
            cx.emit(BufferEvent::FileHandleChanged);
            cx.notify();
        }
    }

    pub fn base_buffer(&self) -> Option<Entity<Self>> {
        Some(self.branch_state.as_ref()?.base_buffer.clone())
    }

    /// Returns the primary [`Language`] assigned to this [`Buffer`].
    pub fn language(&self) -> Option<&Arc<Language>> {
        self.language.as_ref()
    }

    /// Returns the [`Language`] at the given location.
    pub fn language_at<D: ToOffset>(&self, position: D) -> Option<Arc<Language>> {
        let offset = position.to_offset(self);
        let text: &TextBufferSnapshot = &self.text;
        self.syntax_map
            .lock()
            .layers_for_range(offset..offset, text, false)
            .filter(|layer| {
                layer
                    .included_sub_ranges
                    .is_none_or(|ranges| offset_in_sub_ranges(ranges, offset, text))
            })
            .last()
            .map(|info| info.language.clone())
            .or_else(|| self.language.clone())
    }

    /// Returns each [`Language`] for the active syntax layers at the given location.
    pub fn languages_at<D: ToOffset>(&self, position: D) -> Vec<Arc<Language>> {
        let offset = position.to_offset(self);
        let text: &TextBufferSnapshot = &self.text;
        let mut languages: Vec<Arc<Language>> = self
            .syntax_map
            .lock()
            .layers_for_range(offset..offset, text, false)
            .filter(|layer| {
                // For combined injections, check if offset is within the actual sub-ranges.
                layer
                    .included_sub_ranges
                    .is_none_or(|ranges| offset_in_sub_ranges(ranges, offset, text))
            })
            .map(|info| info.language.clone())
            .collect();

        if languages.is_empty()
            && let Some(buffer_language) = self.language()
        {
            languages.push(buffer_language.clone());
        }

        languages
    }

    /// An integer version number that accounts for all updates besides
    /// the buffer's text itself (which is versioned via a version vector).
    pub fn non_text_state_update_count(&self) -> usize {
        self.non_text_state_update_count
    }

    /// Whether the buffer is being parsed in the background.
    #[cfg(any(test, feature = "test-support"))]
    pub fn is_parsing(&self) -> bool {
        self.reparse.is_some()
    }

    /// Indicates whether the buffer contains any regions that may be
    /// written in a language that hasn't been loaded yet.
    pub fn contains_unknown_injections(&self) -> bool {
        self.syntax_map.lock().contains_unknown_injections()
    }

    /// Sets the sync parse timeout for this buffer.
    ///
    /// Setting this to `None` disables sync parsing entirely.
    pub fn set_sync_parse_timeout(&mut self, timeout: Option<Duration>) {
        self.sync_parse_timeout = timeout;
    }

    pub(super) fn invalidate_tree_sitter_data(
        tree_sitter_data: &mut Arc<TreeSitterData>,
        snapshot: &text::BufferSnapshot,
    ) {
        match Arc::get_mut(tree_sitter_data) {
            Some(tree_sitter_data) => tree_sitter_data.clear(snapshot),
            None => {
                let new_tree_sitter_data = TreeSitterData::new(snapshot);
                *tree_sitter_data = Arc::new(new_tree_sitter_data)
            }
        }
    }
}
