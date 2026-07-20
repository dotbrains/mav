use super::*;

impl Pane {
    pub async fn save_item(
        project: Entity<Project>,
        pane: &WeakEntity<Pane>,
        item: &dyn ItemHandle,
        save_intent: SaveIntent,
        cx: &mut AsyncWindowContext,
    ) -> Result<bool> {
        const CONFLICT_MESSAGE: &str = "This file has changed on disk since you started editing it. Do you want to overwrite it?";

        const DELETED_MESSAGE: &str = "This file has been deleted on disk since you started editing it. Do you want to recreate it?";

        let path_style = project.read_with(cx, |project, cx| project.path_style(cx));
        if save_intent == SaveIntent::Skip {
            let is_saveable_singleton = cx.update(|_window, cx| {
                item.can_save(cx) && item.buffer_kind(cx) == ItemBufferKind::Singleton
            })?;
            if is_saveable_singleton {
                pane.update_in(cx, |_, window, cx| item.reload(project, window, cx))?
                    .await
                    .log_err();
            }
            return Ok(true);
        };
        let Some(item_ix) = pane
            .read_with(cx, |pane, _| pane.index_for_item(item))
            .ok()
            .flatten()
        else {
            return Ok(true);
        };

        let (
            mut has_conflict,
            mut is_dirty,
            mut can_save,
            can_save_as,
            is_singleton,
            has_deleted_file,
        ) = cx.update(|_window, cx| {
            (
                item.has_conflict(cx),
                item.is_dirty(cx),
                item.can_save(cx),
                item.can_save_as(cx),
                item.buffer_kind(cx) == ItemBufferKind::Singleton,
                item.has_deleted_file(cx),
            )
        })?;

        // when saving a single buffer, we ignore whether or not it's dirty.
        if save_intent == SaveIntent::Save
            || save_intent == SaveIntent::FormatAndSave
            || save_intent == SaveIntent::SaveWithoutFormat
        {
            is_dirty = true;
        }

        if save_intent == SaveIntent::SaveAs {
            is_dirty = true;
            has_conflict = false;
            can_save = false;
        }

        if save_intent == SaveIntent::Overwrite {
            has_conflict = false;
        }

        let should_format = save_intent != SaveIntent::SaveWithoutFormat;
        let force_format = save_intent == SaveIntent::FormatAndSave;

        if has_conflict && can_save {
            if has_deleted_file && is_singleton {
                let answer = pane.update_in(cx, |pane, window, cx| {
                    pane.activate_item(item_ix, true, true, window, cx);
                    window.prompt(
                        PromptLevel::Warning,
                        DELETED_MESSAGE,
                        None,
                        &["Save", "Close", "Cancel"],
                        cx,
                    )
                })?;
                match answer.await {
                    Ok(0) => {
                        pane.update_in(cx, |_, window, cx| {
                            item.save(
                                SaveOptions {
                                    format: should_format,
                                    force_format,
                                    autosave: false,
                                },
                                project,
                                window,
                                cx,
                            )
                        })?
                        .await?
                    }
                    Ok(1) => {
                        pane.update_in(cx, |pane, window, cx| {
                            pane.remove_item(item.item_id(), false, true, window, cx)
                        })?;
                    }
                    _ => return Ok(false),
                }
                return Ok(true);
            } else {
                let answer = pane.update_in(cx, |pane, window, cx| {
                    pane.activate_item(item_ix, true, true, window, cx);
                    window.prompt(
                        PromptLevel::Warning,
                        CONFLICT_MESSAGE,
                        None,
                        &["Overwrite", "Discard", "Cancel"],
                        cx,
                    )
                })?;
                match answer.await {
                    Ok(0) => {
                        pane.update_in(cx, |_, window, cx| {
                            item.save(
                                SaveOptions {
                                    format: should_format,
                                    force_format,
                                    autosave: false,
                                },
                                project,
                                window,
                                cx,
                            )
                        })?
                        .await?
                    }
                    Ok(1) => {
                        pane.update_in(cx, |_, window, cx| item.reload(project, window, cx))?
                            .await?
                    }
                    _ => return Ok(false),
                }
            }
        } else if is_dirty && (can_save || can_save_as) {
            if save_intent == SaveIntent::Close {
                let will_autosave = cx.update(|_window, cx| {
                    item.can_autosave(cx)
                        && item.workspace_settings(cx).autosave.should_save_on_close()
                })?;
                if !will_autosave {
                    let item_id = item.item_id();
                    let answer_task = pane.update_in(cx, |pane, window, cx| {
                        if pane.save_modals_spawned.insert(item_id) {
                            pane.activate_item(item_ix, true, true, window, cx);
                            let prompt = dirty_message_for(item.project_path(cx), path_style);
                            Some(window.prompt(
                                PromptLevel::Warning,
                                &prompt,
                                None,
                                &["Save", "Don't Save", "Cancel"],
                                cx,
                            ))
                        } else {
                            None
                        }
                    })?;
                    if let Some(answer_task) = answer_task {
                        let answer = answer_task.await;
                        pane.update(cx, |pane, _| {
                        if !pane.save_modals_spawned.remove(&item_id) {
                            debug_panic!(
                                "save modal was not present in spawned modals after awaiting for its answer"
                            )
                        }
                    })?;
                        match answer {
                            Ok(0) => {}
                            Ok(1) => {
                                // Don't save this file - reload from disk to discard changes
                                pane.update_in(cx, |pane, _, cx| {
                                    if pane.is_tab_pinned(item_ix) && !item.can_save(cx) {
                                        pane.pinned_tab_count -= 1;
                                    }
                                })
                                .log_err();
                                if can_save && is_singleton {
                                    pane.update_in(cx, |_, window, cx| {
                                        item.reload(project.clone(), window, cx)
                                    })?
                                    .await
                                    .log_err();
                                }
                                return Ok(true);
                            }
                            _ => return Ok(false), // Cancel
                        }
                    } else {
                        return Ok(false);
                    }
                }
            }

            if can_save {
                pane.update_in(cx, |pane, window, cx| {
                    pane.unpreview_item_if_preview(item.item_id());
                    item.save(
                        SaveOptions {
                            format: should_format,
                            force_format,
                            autosave: false,
                        },
                        project,
                        window,
                        cx,
                    )
                })?
                .await?;
            } else if can_save_as && is_singleton {
                let suggested_name =
                    cx.update(|_window, cx| item.suggested_filename(cx).to_string())?;
                let new_path = pane.update_in(cx, |pane, window, cx| {
                    pane.activate_item(item_ix, true, true, window, cx);
                    pane.workspace.update(cx, |workspace, cx| {
                        let lister = if workspace.project().read(cx).is_local() {
                            DirectoryLister::Local(
                                workspace.project().clone(),
                                workspace.app_state().fs.clone(),
                            )
                        } else {
                            DirectoryLister::Project(workspace.project().clone())
                        };
                        workspace.prompt_for_new_path(lister, Some(suggested_name), window, cx)
                    })
                })??;
                let Some(new_path) = new_path.await.ok().flatten().into_iter().flatten().next()
                else {
                    return Ok(false);
                };

                let project_path = pane
                    .update(cx, |pane, cx| {
                        pane.project
                            .update(cx, |project, cx| {
                                project.find_or_create_worktree(new_path, true, cx)
                            })
                            .ok()
                    })
                    .ok()
                    .flatten();
                let save_task = if let Some(project_path) = project_path {
                    let (worktree, path) = project_path.await?;
                    let worktree_id = worktree.read_with(cx, |worktree, _| worktree.id());
                    let new_path = ProjectPath { worktree_id, path };

                    pane.update_in(cx, |pane, window, cx| {
                        if let Some(item) = pane.item_for_path(new_path.clone(), cx) {
                            pane.remove_item(item.item_id(), false, false, window, cx);
                        }

                        item.save_as(project.clone(), new_path, window, cx)
                    })?
                } else {
                    return Ok(false);
                };

                save_task.await?;
                if should_format {
                    pane.update_in(cx, |pane, window, cx| {
                        pane.unpreview_item_if_preview(item.item_id());
                        item.save(
                            SaveOptions {
                                format: true,
                                autosave: false,
                                force_format,
                            },
                            project,
                            window,
                            cx,
                        )
                    })?
                    .await?;
                }
                return Ok(true);
            }
        }

        pane.update(cx, |_, cx| {
            cx.emit(Event::UserSavedItem {
                item: item.downgrade_item(),
                save_intent,
            });
            true
        })
    }

    pub fn autosave_item(
        item: &dyn ItemHandle,
        project: Entity<Project>,
        window: &mut Window,
        cx: &mut App,
    ) -> Task<Result<()>> {
        let format = !matches!(
            item.workspace_settings(cx).autosave,
            AutosaveSetting::AfterDelay { .. }
        );
        if item.can_autosave(cx) {
            item.save(
                SaveOptions {
                    format,
                    force_format: false,
                    autosave: true,
                },
                project,
                window,
                cx,
            )
        } else {
            Task::ready(Ok(()))
        }
    }
}
