use super::*;

impl Editor {
    /// Inserted text is normalized to LF line endings before being applied.
    /// Normalize before measuring inserted text for post-edit offsets.
    pub fn edit<I, S, T>(&mut self, edits: I, cx: &mut Context<Self>)
    where
        I: IntoIterator<Item = (Range<S>, T)>,
        S: ToOffset,
        T: Into<Arc<str>>,
    {
        if self.read_only(cx) {
            return;
        }

        self.buffer
            .update(cx, |buffer, cx| buffer.edit(edits, None, cx));
    }

    pub fn edit_with_autoindent<I, S, T>(&mut self, edits: I, cx: &mut Context<Self>)
    where
        I: IntoIterator<Item = (Range<S>, T)>,
        S: ToOffset,
        T: Into<Arc<str>>,
    {
        if self.read_only(cx) {
            return;
        }

        self.buffer.update(cx, |buffer, cx| {
            buffer.edit(edits, self.autoindent_mode.clone(), cx)
        });
    }

    pub fn edit_with_block_indent<I, S, T>(
        &mut self,
        edits: I,
        original_indent_columns: Vec<Option<u32>>,
        cx: &mut Context<Self>,
    ) where
        I: IntoIterator<Item = (Range<S>, T)>,
        S: ToOffset,
        T: Into<Arc<str>>,
    {
        if self.read_only(cx) {
            return;
        }

        self.buffer.update(cx, |buffer, cx| {
            buffer.edit(
                edits,
                Some(AutoindentMode::Block {
                    original_indent_columns,
                }),
                cx,
            )
        });
    }

    pub fn cancel(&mut self, _: &Cancel, window: &mut Window, cx: &mut Context<Self>) {
        self.selection_mark_mode = false;
        self.selection_drag_state = SelectionDragState::None;

        if self.dismiss_menus_and_popups(true, window, cx) {
            cx.notify();
            return;
        }
        if self.clear_expanded_diff_hunks(cx) {
            cx.notify();
            return;
        }
        if self.show_git_blame_gutter {
            self.show_git_blame_gutter = false;
            cx.notify();
            return;
        }

        if self.mode.is_full()
            && self.change_selections(Default::default(), window, cx, |s| s.try_cancel())
        {
            cx.notify();
            return;
        }

        cx.propagate();
    }

    pub fn dismiss_menus_and_popups(
        &mut self,
        is_user_requested: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let mut dismissed = false;

        dismissed |= self.take_rename(false, window, cx).is_some();
        dismissed |= self.hide_blame_popover(true, cx);
        dismissed |= hide_hover(self, cx);
        dismissed |= self.hide_signature_help(cx, SignatureHelpHiddenBy::Escape);
        dismissed |= self.hide_context_menu(window, cx).is_some();
        dismissed |= self.mouse_context_menu.take().is_some();
        dismissed |= is_user_requested
            && self.discard_edit_prediction(EditPredictionDiscardReason::Rejected, cx);
        dismissed |= self.snippet_stack.pop().is_some();
        if self.diff_review_drag_state.is_some() {
            self.cancel_diff_review_drag(cx);
            dismissed = true;
        }
        if !self.diff_review_overlays.is_empty() {
            self.dismiss_all_diff_review_overlays(cx);
            dismissed = true;
        }

        if self.mode.is_full() && self.has_active_diagnostic_group() {
            self.dismiss_diagnostics(cx);
            dismissed = true;
        }

        dismissed
    }

    pub(super) fn open_transaction_for_hidden_buffers(
        workspace: Entity<Workspace>,
        transaction: ProjectTransaction,
        title: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if transaction.0.is_empty() {
            return;
        }

        let edited_buffers_already_open = {
            let other_editors: Vec<Entity<Editor>> = workspace
                .read(cx)
                .panes()
                .iter()
                .flat_map(|pane| pane.read(cx).items_of_type::<Editor>())
                .filter(|editor| editor.entity_id() != cx.entity_id())
                .collect();

            transaction.0.keys().all(|buffer| {
                other_editors.iter().any(|editor| {
                    let multi_buffer = editor.read(cx).buffer();
                    multi_buffer.read(cx).is_singleton()
                        && multi_buffer
                            .read(cx)
                            .as_singleton()
                            .map_or(false, |singleton| {
                                singleton.entity_id() == buffer.entity_id()
                            })
                })
            })
        };
        if !edited_buffers_already_open {
            let workspace = workspace.downgrade();
            cx.defer_in(window, move |_, window, cx| {
                cx.spawn_in(window, async move |editor, cx| {
                    Self::open_project_transaction(&editor, workspace, transaction, title, cx)
                        .await
                        .ok()
                })
                .detach();
            });
        }
    }

    pub async fn open_project_transaction(
        editor: &WeakEntity<Editor>,
        workspace: WeakEntity<Workspace>,
        transaction: ProjectTransaction,
        title: String,
        cx: &mut AsyncWindowContext,
    ) -> Result<()> {
        let mut entries = transaction.0.into_iter().collect::<Vec<_>>();
        cx.update(|_, cx| {
            entries.sort_unstable_by_key(|(buffer, _)| {
                buffer.read(cx).file().map(|f| f.path().clone())
            });
        })?;
        if entries.is_empty() {
            return Ok(());
        }

        // If the project transaction's edits are all contained within this editor, then
        // avoid opening a new editor to display them.

        if let [(buffer, transaction)] = &*entries {
            let cursor_excerpt = editor.update(cx, |editor, cx| {
                let snapshot = editor.buffer().read(cx).snapshot(cx);
                let head = editor.selections.newest_anchor().head();
                let (buffer_snapshot, excerpt_range) = snapshot.excerpt_containing(head..head)?;
                if buffer_snapshot.remote_id() != buffer.read(cx).remote_id() {
                    return None;
                }
                Some(excerpt_range)
            })?;

            if let Some(excerpt_range) = cursor_excerpt {
                let all_edits_within_excerpt = buffer.read_with(cx, |buffer, _| {
                    let excerpt_range = excerpt_range.context.to_offset(buffer);
                    buffer
                        .edited_ranges_for_transaction::<usize>(transaction)
                        .all(|range| {
                            excerpt_range.start <= range.start && excerpt_range.end >= range.end
                        })
                });

                if all_edits_within_excerpt {
                    return Ok(());
                }
            }
        }

        let mut ranges_to_highlight = Vec::new();
        let excerpt_buffer = cx.new(|cx| {
            let mut multibuffer = MultiBuffer::new(Capability::ReadWrite).with_title(title);
            for (buffer_handle, transaction) in &entries {
                let edited_ranges = buffer_handle
                    .read(cx)
                    .edited_ranges_for_transaction::<Point>(transaction)
                    .collect::<Vec<_>>();
                multibuffer.set_excerpts_for_path(
                    PathKey::for_buffer(buffer_handle, cx),
                    buffer_handle.clone(),
                    edited_ranges.clone(),
                    multibuffer_context_lines(cx),
                    cx,
                );
                let snapshot = multibuffer.snapshot(cx);
                let buffer_snapshot = buffer_handle.read(cx).snapshot();
                ranges_to_highlight.extend(edited_ranges.into_iter().filter_map(|range| {
                    let text_range = buffer_snapshot.anchor_range_inside(range);
                    let start = snapshot.anchor_in_buffer(text_range.start)?;
                    let end = snapshot.anchor_in_buffer(text_range.end)?;
                    Some(start..end)
                }));
            }
            multibuffer.push_transaction(entries.iter().map(|(b, t)| (b, t)), cx);
            multibuffer
        });

        workspace.update_in(cx, |workspace, window, cx| {
            let project = workspace.project().clone();
            let editor =
                cx.new(|cx| Editor::for_multibuffer(excerpt_buffer, Some(project), window, cx));
            workspace.add_item_to_active_pane(Box::new(editor.clone()), None, true, window, cx);
            editor.update(cx, |editor, cx| {
                editor.highlight_background(
                    HighlightKey::Editor,
                    &ranges_to_highlight,
                    |_, theme| theme.colors().editor_highlighted_line_background,
                    cx,
                );
            });
        })?;

        Ok(())
    }
}
