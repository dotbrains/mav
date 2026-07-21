use super::*;

impl ThreadView {
    pub(super) fn queue_message(
        &mut self,
        message_editor: Entity<MessageEditor>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let is_idle = self.thread.read(cx).status() == acp_thread::ThreadStatus::Idle;

        if is_idle {
            self.send_impl(message_editor, window, cx);
            return;
        }

        let contents = self.resolve_message_contents(&message_editor, cx);

        cx.spawn_in(window, async move |this, cx| {
            let (content, tracked_buffers) = contents.await?;

            if content.is_empty() {
                return Ok::<(), anyhow::Error>(());
            }

            this.update_in(cx, |this, window, cx| {
                this.add_to_queue(content, tracked_buffers, window, cx);
                message_editor.update(cx, |message_editor, cx| {
                    message_editor.clear(window, cx);
                });
                cx.notify();
            })?;
            Ok(())
        })
        .detach_and_log_err(cx);
    }

    pub fn add_to_queue(
        &mut self,
        content: Vec<acp::ContentBlock>,
        tracked_buffers: Vec<Entity<Buffer>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // The ID must be allocated up front so the editor event subscription
        // can capture it before the entry (which owns the subscription) exists.
        let id = self.message_queue.next_id();

        let editor = cx.new(|cx| {
            let mut editor = MessageEditor::new(
                self.workspace.clone(),
                self.project.clone(),
                None,
                self.session_capabilities.clone(),
                self.agent_id.clone(),
                "",
                EditorMode::AutoHeight {
                    min_lines: 1,
                    max_lines: Some(10),
                },
                window,
                cx,
            );
            editor.set_read_only(true, cx);
            editor.set_message(content.clone(), window, cx);
            editor
        });

        let subscription =
            cx.subscribe_in(&editor, window, move |this, _editor, event, window, cx| {
                this.handle_queue_editor_event(id, event, window, cx);
            });

        self.message_queue.enqueue(QueueEntry {
            id,
            content,
            tracked_buffers,
            steer: false,
            editor,
            _subscription: subscription,
        });
        self.sync_queue_flag_to_native_thread(cx);
        cx.notify();
    }

    fn handle_queue_editor_event(
        &mut self,
        id: QueueEntryId,
        event: &MessageEditorEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            MessageEditorEvent::InputAttempted {
                attempt,
                cursor_offset,
            } => {
                self.move_queued_message_to_main_editor(
                    id,
                    Some(attempt.clone()),
                    Some(*cursor_offset),
                    window,
                    cx,
                );
            }
            MessageEditorEvent::LostFocus => {
                self.save_queued_message(id, cx);
            }
            MessageEditorEvent::Cancel | MessageEditorEvent::Send => {
                window.focus(&self.message_editor.focus_handle(cx), cx);
            }
            MessageEditorEvent::SendImmediately => {
                self.send_queued_message_now(id, window, cx);
            }
            _ => {}
        }
    }

    fn save_queued_message(&mut self, id: QueueEntryId, cx: &mut Context<Self>) {
        let Some(entry) = self.message_queue.entry_by_id(id) else {
            return;
        };
        let contents_task = entry
            .editor
            .update(cx, |editor, cx| editor.contents(false, cx));

        cx.spawn(async move |this, cx| {
            let (content, tracked_buffers) = contents_task.await?;

            this.update(cx, |this, cx| {
                if let Some(entry) = this.message_queue.entry_by_id_mut(id) {
                    entry.content = content;
                    entry.tracked_buffers = tracked_buffers;
                }
                cx.notify();
            })?;

            Ok::<(), anyhow::Error>(())
        })
        .detach_and_log_err(cx);
    }

    pub fn remove_from_queue(
        &mut self,
        id: QueueEntryId,
        cx: &mut Context<Self>,
    ) -> Option<QueueEntry> {
        let removed = self.message_queue.remove(id);
        if removed.is_some() {
            self.sync_queue_flag_to_native_thread(cx);
        }
        removed
    }

    pub(super) fn toggle_queue_entry_steer(&mut self, id: QueueEntryId, cx: &mut Context<Self>) {
        self.message_queue.toggle_steer(id);
        self.sync_queue_flag_to_native_thread(cx);
        cx.notify();
    }

    pub fn sync_queue_flag_to_native_thread(&self, cx: &mut Context<Self>) {
        if let Some(native_thread) = self.as_native_thread(cx) {
            // By default queued messages wait for the turn to fully complete.
            // Only a "steering" front message ends the turn at the next boundary.
            let end_at_boundary = self.message_queue.front_wants_steer();
            native_thread.update(cx, |thread, _| {
                thread.set_end_turn_at_next_boundary(end_at_boundary);
            });
        }
    }

    pub fn send_queued_message_now(
        &mut self,
        id: QueueEntryId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let is_generating = self.thread.read(cx).status() == acp_thread::ThreadStatus::Generating;
        if let Some(entry) = self.message_queue.send_now(id, is_generating) {
            self.dispatch_queued_entry(entry, window, cx);
        }
    }

    /// The shared "actually send this entry" path, used by fast-track,
    /// auto-processing on Stopped, and "Send Now". The entry must already have
    /// been removed from the queue.
    pub fn dispatch_queued_entry(
        &mut self,
        entry: QueueEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.sync_queue_flag_to_native_thread(cx);

        cx.emit(AcpThreadViewEvent::Interacted);

        self.message_editor.focus_handle(cx).focus(window, cx);

        let content = entry.content;
        let tracked_buffers = entry.tracked_buffers;

        // A queued message can itself be a built-in command (e.g. the user typed
        // `/compact` while a turn was generating). Detect that so we run it as a
        // command turn without echoing it as a user message, matching the
        // non-queued path.
        let is_native_command = content
            .first()
            .and_then(|block| match block {
                acp::ContentBlock::Text(text) => Some(text.text.as_str()),
                _ => None,
            })
            .and_then(|text| {
                leading_native_command(text, self.session_capabilities.read().available_commands())
            })
            .is_some();

        let cancelled = self.thread.update(cx, |thread, cx| thread.cancel(cx));

        let workspace = self.workspace.clone();

        let should_be_following = self.should_be_following;
        let contents_task = cx.spawn_in(window, async move |_this, cx| {
            cancelled.await;
            if should_be_following {
                workspace
                    .update_in(cx, |workspace, window, cx| {
                        workspace.follow(CollaboratorId::Agent, window, cx);
                    })
                    .ok();
            }

            Ok(Some((content, tracked_buffers)))
        });

        self.send_content(contents_task, is_native_command, window, cx);
    }

    pub fn move_queued_message_to_main_editor(
        &mut self,
        id: QueueEntryId,
        attempt: Option<InputAttempt>,
        cursor_offset: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(queued_message) = self.remove_from_queue(id, cx) else {
            return false;
        };
        let queued_content = queued_message.content;
        let message_editor = self.message_editor.clone();

        window.focus(&message_editor.focus_handle(cx), cx);

        let adjusted_cursor_offset = if message_editor.read(cx).is_empty(cx) {
            message_editor.update(cx, |editor, cx| {
                editor.set_message(queued_content, window, cx);
            });
            cursor_offset
        } else {
            let existing_len = message_editor.read(cx).text(cx).len();
            let separator = "\n\n";
            message_editor.update(cx, |editor, cx| {
                editor.append_message(queued_content, Some(separator), window, cx);
            });
            cursor_offset.map(|offset| existing_len + separator.len() + offset)
        };

        message_editor.update(cx, |editor, cx| {
            if let Some(offset) = adjusted_cursor_offset {
                editor.set_cursor_offset(offset, window, cx);
            }
            match attempt {
                Some(InputAttempt::Text(text)) => {
                    editor.insert_text(&text, window, cx);
                }
                Some(InputAttempt::Paste(clipboard)) => {
                    editor.paste_item(&clipboard, window, cx);
                }
                None => {}
            }
        });

        cx.notify();
        true
    }

    pub(super) fn handle_message_editor_move_up(
        &mut self,
        _: &mav_actions::editor::MoveUp,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.message_editor.read(cx).is_empty(cx) {
            cx.propagate();
            return;
        }
        let Some(last_id) = self.message_queue.last_id() else {
            cx.propagate();
            return;
        };
        self.move_queued_message_to_main_editor(last_id, None, None, window, cx);
    }
}
