use super::*;

impl AcpThread {
    /// Restores the git working tree to the state at the given checkpoint (if one exists)
    pub fn restore_checkpoint(
        &mut self,
        client_id: ClientUserMessageId,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        let Some((_, message)) = self.user_message_mut(&client_id) else {
            return Task::ready(Err(anyhow!("message not found")));
        };

        let checkpoint = message
            .checkpoint
            .as_ref()
            .map(|c| c.git_checkpoint.clone());

        // Cancel any in-progress generation before restoring
        let cancel_task = self.cancel(cx);
        let rewind = self.rewind(client_id.clone(), cx);
        let git_store = self.project.read(cx).git_store().clone();

        cx.spawn(async move |_, cx| {
            cancel_task.await;
            rewind.await?;
            if let Some(checkpoint) = checkpoint {
                git_store
                    .update(cx, |git, cx| git.restore_checkpoint(checkpoint, cx))
                    .await?;
            }

            Ok(())
        })
    }

    /// Rewinds this thread to before the entry at `index`, removing it and all
    /// subsequent entries while rejecting any action_log changes made from that point.
    /// Unlike `restore_checkpoint`, this method does not restore from git.
    pub fn rewind(
        &mut self,
        client_id: ClientUserMessageId,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        let Some(truncate) = self.connection.truncate(&self.session_id, cx) else {
            return Task::ready(Err(anyhow!("not supported")));
        };

        Self::flush_streaming_text(&mut self.streaming_text_buffer, cx);
        let telemetry = ActionLogTelemetry::from(&*self);
        cx.spawn(async move |this, cx| {
            cx.update(|cx| truncate.run(client_id.clone(), cx)).await?;
            this.update(cx, |this, cx| {
                if let Some((ix, _)) = this.user_message_mut(&client_id) {
                    // Collect all terminals from entries that will be removed
                    let terminals_to_remove: Vec<acp::TerminalId> = this.entries[ix..]
                        .iter()
                        .flat_map(|entry| entry.terminals())
                        .filter_map(|terminal| terminal.read(cx).id().clone().into())
                        .collect();

                    let range = ix..this.entries.len();
                    this.entries.truncate(ix);
                    cx.emit(AcpThreadEvent::EntriesRemoved(range));

                    // Kill and remove the terminals
                    for terminal_id in terminals_to_remove {
                        if let Some(terminal) = this.terminals.remove(&terminal_id) {
                            terminal.update(cx, |terminal, cx| {
                                terminal.kill(cx);
                            });
                        }
                    }
                }
                this.action_log().update(cx, |action_log, cx| {
                    action_log.reject_all_edits(Some(telemetry), cx)
                })
            })?
            .await;
            Ok(())
        })
    }

    pub(super) fn update_last_checkpoint_if_changed(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        let Some(turn_id) = self.running_turn.as_ref().map(|turn| turn.id) else {
            return Task::ready(Ok(()));
        };

        let git_store = self.project.read(cx).git_store().clone();

        let Some((client_id, checkpoint)) = self.last_user_message().and_then(|(_, message)| {
            let id = message.client_id.clone()?;
            let checkpoint = message.checkpoint.as_ref()?;
            Some((id, checkpoint))
        }) else {
            return Task::ready(Ok(()));
        };
        if checkpoint.show {
            return Task::ready(Ok(()));
        }
        let old_checkpoint = checkpoint.git_checkpoint.clone();

        let new_checkpoint = git_store.update(cx, |git, cx| git.checkpoint(cx));
        cx.spawn(async move |this, cx| {
            let Some(new_checkpoint) = new_checkpoint
                .await
                .context("failed to get new checkpoint")
                .log_err()
            else {
                return Ok(());
            };

            let Some(equal) = git_store
                .update(cx, |git, cx| {
                    git.compare_checkpoints(old_checkpoint.clone(), new_checkpoint, cx)
                })
                .await
                .context("failed to compare checkpoints")
                .log_err()
            else {
                return Ok(());
            };

            if equal {
                return Ok(());
            }

            this.update(cx, |this, cx| {
                if !this
                    .running_turn
                    .as_ref()
                    .is_some_and(|turn| turn.id == turn_id)
                {
                    return;
                }

                let Some((ix, message)) = this.last_user_message() else {
                    return;
                };
                if message.client_id.as_ref() != Some(&client_id) {
                    return;
                }
                if let Some(checkpoint) = message.checkpoint.as_mut()
                    && !checkpoint.show
                {
                    checkpoint.show = true;
                    cx.emit(AcpThreadEvent::EntryUpdated(ix));
                }
            })?;

            Ok(())
        })
    }

    pub(super) fn update_last_checkpoint(&mut self, cx: &mut Context<Self>) -> Task<Result<()>> {
        let git_store = self.project.read(cx).git_store().clone();

        let Some((_, message)) = self.last_user_message() else {
            return Task::ready(Ok(()));
        };
        let Some(client_id) = message.client_id.clone() else {
            return Task::ready(Ok(()));
        };
        let Some(checkpoint) = message.checkpoint.as_ref() else {
            return Task::ready(Ok(()));
        };
        let old_checkpoint = checkpoint.git_checkpoint.clone();

        let new_checkpoint = git_store.update(cx, |git, cx| git.checkpoint(cx));
        cx.spawn(async move |this, cx| {
            let Some(new_checkpoint) = new_checkpoint
                .await
                .context("failed to get new checkpoint")
                .log_err()
            else {
                return Ok(());
            };

            let equal = git_store
                .update(cx, |git, cx| {
                    git.compare_checkpoints(old_checkpoint.clone(), new_checkpoint, cx)
                })
                .await
                .unwrap_or(true);

            this.update(cx, |this, cx| {
                if let Some((ix, message)) = this.user_message_mut(&client_id) {
                    if let Some(checkpoint) = message.checkpoint.as_mut() {
                        checkpoint.show = !equal;
                        cx.emit(AcpThreadEvent::EntryUpdated(ix));
                    }
                }
            })?;

            Ok(())
        })
    }

    pub(super) fn last_user_message(&mut self) -> Option<(usize, &mut UserMessage)> {
        self.entries
            .iter_mut()
            .enumerate()
            .rev()
            .find_map(|(ix, entry)| {
                if let AgentThreadEntry::UserMessage(message) = entry {
                    Some((ix, message))
                } else {
                    None
                }
            })
    }

    pub(super) fn user_message_mut(
        &mut self,
        client_id: &ClientUserMessageId,
    ) -> Option<(usize, &mut UserMessage)> {
        self.entries.iter_mut().enumerate().find_map(|(ix, entry)| {
            if let AgentThreadEntry::UserMessage(message) = entry {
                if message.client_id.as_ref() == Some(client_id) {
                    Some((ix, message))
                } else {
                    None
                }
            } else {
                None
            }
        })
    }
}
