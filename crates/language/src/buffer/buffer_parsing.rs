use super::*;

impl Buffer {
    /// Called after an edit to synchronize the buffer's main parse tree with
    /// the buffer's new underlying state.
    ///
    /// Locks the syntax map and interpolates the edits since the last reparse
    /// into the foreground syntax tree.
    ///
    /// Then takes a stable snapshot of the syntax map before unlocking it.
    /// The snapshot with the interpolated edits is sent to a background thread,
    /// where we ask Tree-sitter to perform an incremental parse.
    ///
    /// Meanwhile, in the foreground if `may_block` is true, we block the main
    /// thread for up to 1ms waiting on the parse to complete. As soon as it
    /// completes, we proceed synchronously, unless a 1ms timeout elapses.
    ///
    /// If we time out waiting on the parse, we spawn a second task waiting
    /// until the parse does complete and return with the interpolated tree still
    /// in the foreground. When the background parse completes, call back into
    /// the main thread and assign the foreground parse state.
    ///
    /// If the buffer or grammar changed since the start of the background parse,
    /// initiate an additional reparse recursively. To avoid concurrent parses
    /// for the same buffer, we only initiate a new parse if we are not already
    /// parsing in the background.
    #[ztracing::instrument(skip_all)]
    pub fn reparse(&mut self, cx: &mut Context<Self>, may_block: bool) {
        if self.text.version() != *self.tree_sitter_data.version() {
            Self::invalidate_tree_sitter_data(&mut self.tree_sitter_data, self.text.snapshot());
        }
        if self.reparse.is_some() {
            return;
        }
        let language = if let Some(language) = self.language.clone() {
            language
        } else {
            return;
        };

        let text = self.text_snapshot();
        let parsed_version = self.version();

        let mut syntax_map = self.syntax_map.lock();
        syntax_map.interpolate(&text);
        let language_registry = syntax_map.language_registry();
        let mut syntax_snapshot = syntax_map.snapshot();
        drop(syntax_map);

        self.parse_status.0.send(ParseStatus::Parsing).unwrap();
        if may_block && let Some(sync_parse_timeout) = self.sync_parse_timeout {
            if let Ok(()) = syntax_snapshot.reparse_with_timeout(
                &text,
                language_registry.clone(),
                language.clone(),
                sync_parse_timeout,
            ) {
                self.did_finish_parsing(syntax_snapshot, Some(Duration::from_millis(300)), cx);
                self.reparse = None;
                return;
            }
        }

        let parse_task = cx.background_spawn({
            let language = language.clone();
            let language_registry = language_registry.clone();
            async move {
                syntax_snapshot.reparse(&text, language_registry, language);
                syntax_snapshot
            }
        });

        self.reparse = Some(cx.spawn(async move |this, cx| {
            let new_syntax_map = parse_task.await;
            this.update(cx, move |this, cx| {
                let grammar_changed = || {
                    this.language
                        .as_ref()
                        .is_none_or(|current_language| !Arc::ptr_eq(&language, current_language))
                };
                let language_registry_changed = || {
                    new_syntax_map.contains_unknown_injections()
                        && language_registry.is_some_and(|registry| {
                            registry.version() != new_syntax_map.language_registry_version()
                        })
                };
                let parse_again = this.version.changed_since(&parsed_version)
                    || language_registry_changed()
                    || grammar_changed();
                this.did_finish_parsing(new_syntax_map, None, cx);
                this.reparse = None;
                if parse_again {
                    this.reparse(cx, false);
                }
            })
            .ok();
        }));
    }

    pub(super) fn did_finish_parsing(
        &mut self,
        syntax_snapshot: SyntaxSnapshot,
        block_budget: Option<Duration>,
        cx: &mut Context<Self>,
    ) {
        self.non_text_state_update_count += 1;
        self.syntax_map.lock().did_parse(syntax_snapshot);
        self.was_changed();
        self.request_autoindent(cx, block_budget);
        self.parse_status.0.send(ParseStatus::Idle).unwrap();
        Self::invalidate_tree_sitter_data(&mut self.tree_sitter_data, &self.text.snapshot());
        cx.emit(BufferEvent::Reparsed);
        cx.notify();
    }

    pub fn parse_status(&self) -> watch::Receiver<ParseStatus> {
        self.parse_status.1.clone()
    }

    /// Wait until the buffer is no longer parsing
    pub fn parsing_idle(&self) -> impl Future<Output = ()> + use<> {
        let mut parse_status = self.parse_status();
        async move {
            while *parse_status.borrow() != ParseStatus::Idle {
                if parse_status.changed().await.is_err() {
                    break;
                }
            }
        }
    }

    /// Assign to the buffer a set of diagnostics created by a given language server.
    pub fn update_diagnostics(
        &mut self,
        server_id: LanguageServerId,
        diagnostics: DiagnosticSet,
        cx: &mut Context<Self>,
    ) {
        let lamport_timestamp = self.text.lamport_clock.tick();
        let op = Operation::UpdateDiagnostics {
            server_id,
            diagnostics: diagnostics.iter().cloned().collect(),
            lamport_timestamp,
        };

        self.apply_diagnostic_update(server_id, diagnostics, lamport_timestamp, cx);
        self.send_operation(op, true, cx);
    }

    pub fn buffer_diagnostics(
        &self,
        for_server: Option<LanguageServerId>,
    ) -> Vec<&DiagnosticEntry<Anchor>> {
        match for_server {
            Some(server_id) => self
                .diagnostics
                .get(&server_id)
                .map_or_else(Vec::new, |diagnostics| diagnostics.iter().collect()),
            None => self
                .diagnostics
                .iter()
                .flat_map(|(_, diagnostic_set)| diagnostic_set.iter())
                .collect(),
        }
    }
}
