use super::*;

impl Workspace {
    pub fn close_global(cx: &mut App) {
        cx.defer(|cx| {
            cx.windows().iter().find(|window| {
                window
                    .update(cx, |_, window, _| {
                        if window.is_window_active() {
                            //This can only get called when the window's project connection has been lost
                            //so we don't need to prompt the user for anything and instead just close the window
                            window.remove_window();
                            true
                        } else {
                            false
                        }
                    })
                    .unwrap_or(false)
            });
        });
    }

    pub fn prepare_to_close(
        &mut self,
        close_intent: CloseIntent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Result<bool>> {
        let active_call = self.active_global_call();

        cx.spawn_in(window, async move |this, cx| {
            this.update(cx, |this, _| {
                if close_intent == CloseIntent::CloseWindow {
                    this.removing = true;
                }
            })?;

            let workspace_count = cx.update(|_window, cx| {
                cx.windows()
                    .iter()
                    .filter(|window| window.downcast::<MultiWorkspace>().is_some())
                    .count()
            })?;

            #[cfg(target_os = "macos")]
            let save_last_workspace = false;

            // On Linux and Windows, closing the last window should restore the last workspace.
            #[cfg(not(target_os = "macos"))]
            let save_last_workspace = {
                let remaining_workspaces = cx.update(|_window, cx| {
                    cx.windows()
                        .iter()
                        .filter_map(|window| window.downcast::<MultiWorkspace>())
                        .filter_map(|multi_workspace| {
                            multi_workspace
                                .update(cx, |multi_workspace, _, cx| {
                                    multi_workspace.workspace().read(cx).removing
                                })
                                .ok()
                        })
                        .filter(|removing| !removing)
                        .count()
                })?;

                close_intent != CloseIntent::ReplaceWindow && remaining_workspaces == 0
            };

            if let Some(active_call) = active_call
                && workspace_count == 1
                && cx
                    .update(|_window, cx| active_call.0.is_in_room(cx))
                    .unwrap_or(false)
            {
                if close_intent == CloseIntent::CloseWindow {
                    this.update(cx, |_, cx| cx.emit(Event::Activate))?;
                    let answer = cx.update(|window, cx| {
                        window.prompt(
                            PromptLevel::Warning,
                            "Do you want to leave the current call?",
                            None,
                            &["Close window and hang up", "Cancel"],
                            cx,
                        )
                    })?;

                    if answer.await.log_err() == Some(1) {
                        return anyhow::Ok(false);
                    } else {
                        if let Ok(task) = cx.update(|_window, cx| active_call.0.hang_up(cx)) {
                            task.await.log_err();
                        }
                    }
                }
                if close_intent == CloseIntent::ReplaceWindow {
                    _ = cx.update(|_window, cx| {
                        let multi_workspace = cx
                            .windows()
                            .iter()
                            .filter_map(|window| window.downcast::<MultiWorkspace>())
                            .next()
                            .unwrap();
                        let project = multi_workspace
                            .read(cx)?
                            .workspace()
                            .read(cx)
                            .project
                            .clone();
                        if project.read(cx).is_shared() {
                            active_call.0.unshare_project(project, cx)?;
                        }
                        Ok::<_, anyhow::Error>(())
                    });
                }
            }

            // Hot-exit silently writes dirty buffers to the DB; only allow it
            // if the workspace will be reachable again, either via session
            // restore or by reopening its folder paths. Otherwise prompt, so
            // we don't orphan the buffers.
            let allow_hot_exit_serialization = close_intent == CloseIntent::Quit
                || save_last_workspace
                || this
                    .read_with(cx, |workspace, cx| {
                        workspace
                            .project
                            .read(cx)
                            .visible_worktrees(cx)
                            .next()
                            .is_some()
                    })
                    .unwrap_or(false);
            let save_result = this
                .update_in(cx, |this, window, cx| {
                    this.save_all_internal(
                        SaveIntent::Close,
                        allow_hot_exit_serialization,
                        window,
                        cx,
                    )
                })?
                .await;

            // If we're not quitting, but closing, we remove the workspace from
            // the current session.
            if close_intent != CloseIntent::Quit
                && !save_last_workspace
                && save_result.as_ref().is_ok_and(|&res| res)
            {
                this.update_in(cx, |this, window, cx| this.remove_from_session(window, cx))?
                    .await;
            }

            save_result
        })
    }

    pub(crate) fn save_all(
        &mut self,
        action: &SaveAll,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.save_all_internal(
            action.save_intent.unwrap_or(SaveIntent::SaveAll),
            true,
            window,
            cx,
        )
        .detach_and_log_err(cx);
    }

    pub(crate) fn send_keystrokes(
        &mut self,
        action: &SendKeystrokes,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let keystrokes: Vec<Keystroke> = action
            .0
            .split(' ')
            .flat_map(|k| Keystroke::parse(k).log_err())
            .map(|k| {
                cx.keyboard_mapper()
                    .map_key_equivalent(k, false)
                    .inner()
                    .clone()
            })
            .collect();
        let _ = self.send_keystrokes_impl(keystrokes, window, cx);
    }

    pub fn send_keystrokes_impl(
        &mut self,
        keystrokes: Vec<Keystroke>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Shared<Task<()>> {
        let mut state = self.dispatching_keystrokes.borrow_mut();
        if !state.dispatched.insert(keystrokes.clone()) {
            cx.propagate();
            return state.task.clone().unwrap();
        }

        state.queue.extend(keystrokes);

        let keystrokes = self.dispatching_keystrokes.clone();
        if state.task.is_none() {
            state.task = Some(
                window
                    .spawn(cx, async move |cx| {
                        // limit to 100 keystrokes to avoid infinite recursion.
                        for _ in 0..100 {
                            let keystroke = {
                                let mut state = keystrokes.borrow_mut();
                                let Some(keystroke) = state.queue.pop_front() else {
                                    state.dispatched.clear();
                                    state.task.take();
                                    return;
                                };
                                keystroke
                            };
                            let focus_changed = cx
                                .update(|window, cx| {
                                    let focused = window.focused(cx);
                                    window.dispatch_keystroke(keystroke.clone(), cx);
                                    if window.focused(cx) != focused {
                                        // dispatch_keystroke may cause the focus to change.
                                        // draw's side effect is to schedule the FocusChanged events in the current flush effect cycle
                                        // And we need that to happen before the next keystroke to keep vim mode happy...
                                        // (Note that the tests always do this implicitly, so you must manually test with something like:
                                        //   "bindings": { "g z": ["workspace::SendKeystrokes", ": j <enter> u"]}
                                        // )
                                        window.draw(cx).clear();
                                        return true;
                                    }
                                    false
                                })
                                .unwrap_or(false);

                            if focus_changed {
                                futures_lite::future::yield_now().await;
                            }
                        }

                        *keystrokes.borrow_mut() = Default::default();
                        log::error!("over 100 keystrokes passed to send_keystrokes");
                    })
                    .shared(),
            );
        }
        state.task.clone().unwrap()
    }

    /// Prompts the user to save or discard each dirty item, returning
    /// `true` if they confirmed (saved/discarded everything) or `false`
    /// if they cancelled. Used before removing worktree roots during
    /// thread archival.
    pub fn prompt_to_save_or_discard_dirty_items(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Result<bool>> {
        self.save_all_internal(SaveIntent::Close, true, window, cx)
    }

    pub(crate) fn save_all_internal(
        &mut self,
        mut save_intent: SaveIntent,
        allow_hot_exit_serialization: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Result<bool>> {
        if self.project.read(cx).is_disconnected(cx) {
            return Task::ready(Ok(true));
        }
        let dirty_items = self
            .panes
            .iter()
            .flat_map(|pane| {
                pane.read(cx).items().filter_map(|item| {
                    if item.is_dirty(cx) {
                        item.tab_content_text(0, cx);
                        Some((pane.downgrade(), item.boxed_clone()))
                    } else {
                        None
                    }
                })
            })
            .collect::<Vec<_>>();

        let project = self.project.clone();
        cx.spawn_in(window, async move |workspace, cx| {
            let dirty_items = if save_intent == SaveIntent::Close && !dirty_items.is_empty() {
                let mut serialize_tasks = Vec::new();
                let mut remaining_dirty_items = Vec::new();
                if allow_hot_exit_serialization {
                    workspace.update_in(cx, |workspace, window, cx| {
                        for (pane, item) in dirty_items {
                            if let Some(task) = item
                                .to_serializable_item_handle(cx)
                                .and_then(|handle| handle.serialize(workspace, true, window, cx))
                            {
                                serialize_tasks.push((pane, item, task));
                            } else {
                                remaining_dirty_items.push((pane, item));
                            }
                        }
                    })?;

                    for (pane, item, task) in serialize_tasks {
                        if task.await.log_err().is_none() {
                            remaining_dirty_items.push((pane, item));
                        }
                    }
                } else {
                    remaining_dirty_items = dirty_items;
                }

                if !remaining_dirty_items.is_empty() {
                    workspace.update(cx, |_, cx| cx.emit(Event::Activate))?;
                }

                if remaining_dirty_items.len() > 1 {
                    let answer = workspace.update_in(cx, |_, window, cx| {
                        cx.emit(Event::Activate);
                        let detail = Pane::file_names_for_prompt(
                            &mut remaining_dirty_items.iter().map(|(_, handle)| handle),
                            cx,
                        );
                        window.prompt(
                            PromptLevel::Warning,
                            "Do you want to save all changes in the following files?",
                            Some(&detail),
                            &["Save all", "Discard all", "Cancel"],
                            cx,
                        )
                    })?;
                    match answer.await.log_err() {
                        Some(0) => save_intent = SaveIntent::SaveAll,
                        Some(1) => save_intent = SaveIntent::Skip,
                        Some(2) => return Ok(false),
                        _ => {}
                    }
                }

                remaining_dirty_items
            } else {
                dirty_items
            };

            for (pane, item) in dirty_items {
                let (singleton, project_entry_ids) = cx.update(|_, cx| {
                    (
                        item.buffer_kind(cx) == ItemBufferKind::Singleton,
                        item.project_entry_ids(cx),
                    )
                })?;
                if (singleton || !project_entry_ids.is_empty())
                    && !Pane::save_item(project.clone(), &pane, &*item, save_intent, cx).await?
                {
                    return Ok(false);
                }
            }
            Ok(true)
        })
    }
}
