use super::*;

impl Editor {
    pub(crate) fn can_format_selections(&self, cx: &App) -> bool {
        if !self.mode.is_full() {
            return false;
        }

        let Some(project) = &self.project else {
            return false;
        };

        let project = project.read(cx);
        let multi_buffer = self.buffer.read(cx);
        let snapshot = multi_buffer.snapshot(cx);

        self.selections
            .disjoint_anchor_ranges()
            .flat_map(|range| [range.start, range.end])
            .filter_map(|anchor| snapshot.anchor_to_buffer_anchor(anchor))
            .filter_map(|(_, buffer_snapshot)| multi_buffer.buffer(buffer_snapshot.remote_id()))
            .any(|buffer| project.supports_range_formatting(&buffer, cx))
    }

    pub(crate) fn format(
        &mut self,
        _: &Format,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Task<Result<()>>> {
        if self.read_only(cx) {
            return None;
        }

        let project = match &self.project {
            Some(project) => project.clone(),
            None => return None,
        };

        Some(self.perform_format(
            project,
            FormatTrigger::Manual,
            FormatTarget::Buffers(self.buffer.read(cx).all_buffers()),
            window,
            cx,
        ))
    }

    pub(crate) fn format_selections(
        &mut self,
        _: &FormatSelections,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Task<Result<()>>> {
        if self.read_only(cx) {
            return None;
        }

        let project = match &self.project {
            Some(project) => project.clone(),
            None => return None,
        };

        let ranges = self
            .selections
            .all_adjusted(&self.display_snapshot(cx))
            .into_iter()
            .map(|selection| selection.range())
            .collect_vec();

        Some(self.perform_format(
            project,
            FormatTrigger::Manual,
            FormatTarget::Ranges(ranges),
            window,
            cx,
        ))
    }

    pub(crate) fn perform_format(
        &mut self,
        project: Entity<Project>,
        trigger: FormatTrigger,
        target: FormatTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        let buffer = self.buffer.clone();
        let (buffers, target) = match target {
            FormatTarget::Buffers(buffers) => (buffers, LspFormatTarget::Buffers),
            FormatTarget::Ranges(selection_ranges) => {
                let multi_buffer = buffer.read(cx);
                let snapshot = multi_buffer.read(cx);
                let mut buffers = HashSet::default();
                let mut buffer_id_to_ranges: BTreeMap<BufferId, Vec<Range<text::Anchor>>> =
                    BTreeMap::new();
                for selection_range in selection_ranges {
                    for (buffer_snapshot, buffer_range, _) in
                        snapshot.range_to_buffer_ranges(selection_range.start..selection_range.end)
                    {
                        let buffer_id = buffer_snapshot.remote_id();
                        let start = buffer_snapshot.anchor_before(buffer_range.start);
                        let end = buffer_snapshot.anchor_after(buffer_range.end);
                        buffers.insert(multi_buffer.buffer(buffer_id).unwrap());
                        buffer_id_to_ranges
                            .entry(buffer_id)
                            .and_modify(|buffer_ranges| buffer_ranges.push(start..end))
                            .or_insert_with(|| vec![start..end]);
                    }
                }
                (buffers, LspFormatTarget::Ranges(buffer_id_to_ranges))
            }
        };

        let transaction_id_prev = buffer.read(cx).last_transaction_id(cx);
        let selections_prev = transaction_id_prev
            .and_then(|transaction_id_prev| {
                // default to selections as they were after the last edit, if we have them,
                // instead of how they are now.
                // This will make it so that editing, moving somewhere else, formatting, then undoing the format
                // will take you back to where you made the last edit, instead of staying where you scrolled
                self.selection_history
                    .transaction(transaction_id_prev)
                    .map(|t| t.0.clone())
            })
            .unwrap_or_else(|| self.selections.disjoint_anchors_arc());

        let mut timeout = cx.background_executor().timer(FORMAT_TIMEOUT).fuse();
        let format = project.update(cx, |project, cx| {
            project.format(buffers, target, true, trigger, cx)
        });

        cx.spawn_in(window, async move |editor, cx| {
            let transaction = futures::select_biased! {
                transaction = format.log_err().fuse() => transaction,
                () = timeout => {
                    log::warn!("timed out waiting for formatting");
                    None
                }
            };

            buffer.update(cx, |buffer, cx| {
                if let Some(transaction) = transaction
                    && !buffer.is_singleton()
                {
                    buffer.push_transaction(&transaction.0, cx);
                }
                cx.notify();
            });

            if let Some(transaction_id_now) =
                buffer.read_with(cx, |b, cx| b.last_transaction_id(cx))
            {
                let has_new_transaction = transaction_id_prev != Some(transaction_id_now);
                if has_new_transaction {
                    editor
                        .update(cx, |editor, _| {
                            editor
                                .selection_history
                                .insert_transaction(transaction_id_now, selections_prev);
                        })
                        .ok();
                }
            }

            Ok(())
        })
    }

    pub(crate) fn organize_imports(
        &mut self,
        _: &OrganizeImports,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Task<Result<()>>> {
        if self.read_only(cx) {
            return None;
        }
        let project = match &self.project {
            Some(project) => project.clone(),
            None => return None,
        };
        Some(self.perform_code_action_kind(
            project,
            CodeActionKind::SOURCE_ORGANIZE_IMPORTS,
            window,
            cx,
        ))
    }

    pub(crate) fn perform_code_action_kind(
        &mut self,
        project: Entity<Project>,
        kind: CodeActionKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        let buffer = self.buffer.clone();
        let buffers = buffer.read(cx).all_buffers();
        let mut timeout = cx.background_executor().timer(CODE_ACTION_TIMEOUT).fuse();
        let apply_action = project.update(cx, |project, cx| {
            project.apply_code_action_kind(buffers, kind, true, cx)
        });
        cx.spawn_in(window, async move |_, cx| {
            let transaction = futures::select_biased! {
                () = timeout => {
                    log::warn!("timed out waiting for executing code action");
                    None
                }
                transaction = apply_action.log_err().fuse() => transaction,
            };
            buffer.update(cx, |buffer, cx| {
                // check if we need this
                if let Some(transaction) = transaction
                    && !buffer.is_singleton()
                {
                    buffer.push_transaction(&transaction.0, cx);
                }
                cx.notify();
            });
            Ok(())
        })
    }
}
