use super::*;

impl SerializableItem for Editor {
    fn serialized_item_kind() -> &'static str {
        "Editor"
    }

    fn cleanup(
        workspace_id: WorkspaceId,
        alive_items: Vec<ItemId>,
        _window: &mut Window,
        cx: &mut App,
    ) -> Task<Result<()>> {
        workspace::delete_unloaded_items(
            alive_items,
            workspace_id,
            "editors",
            &EditorDb::global(cx),
            cx,
        )
    }

    fn deserialize(
        project: Entity<Project>,
        _workspace: WeakEntity<Workspace>,
        workspace_id: workspace::WorkspaceId,
        item_id: ItemId,
        window: &mut Window,
        cx: &mut App,
    ) -> Task<Result<Entity<Self>>> {
        let serialized_editor = match EditorDb::global(cx)
            .get_serialized_editor(item_id, workspace_id)
            .context("Failed to query editor state")
        {
            Ok(Some(serialized_editor)) => {
                if ProjectSettings::get_global(cx)
                    .session
                    .restore_unsaved_buffers
                {
                    serialized_editor
                } else {
                    SerializedEditor {
                        abs_path: serialized_editor.abs_path,
                        contents: None,
                        language: None,
                        mtime: None,
                    }
                }
            }
            Ok(None) => {
                return Task::ready(Err(anyhow!(
                    "Unable to deserialize editor: No entry in database for item_id: {item_id} and workspace_id {workspace_id:?}"
                )));
            }
            Err(error) => {
                return Task::ready(Err(error));
            }
        };
        log::debug!(
            "Deserialized editor {item_id:?} in workspace {workspace_id:?}, {serialized_editor:?}"
        );

        match serialized_editor {
            SerializedEditor {
                abs_path: None,
                contents: Some(contents),
                language,
                ..
            } => window.spawn(cx, {
                let project = project.clone();
                async move |cx| {
                    let language_registry =
                        project.read_with(cx, |project, _| project.languages().clone());

                    let language = if let Some(language_name) = language {
                        // We don't fail here, because we'd rather not set the language if the name changed
                        // than fail to restore the buffer.
                        language_registry
                            .language_for_name(&language_name)
                            .await
                            .ok()
                    } else {
                        None
                    };

                    // First create the empty buffer
                    let buffer = project
                        .update(cx, |project, cx| project.create_buffer(language, true, cx))
                        .await
                        .context("Failed to create buffer while deserializing editor")?;

                    // Then set the text so that the dirty bit is set correctly
                    buffer.update(cx, |buffer, cx| {
                        buffer.set_language_registry(language_registry);
                        buffer.set_text(contents, cx);
                        if let Some(entry) = buffer.peek_undo_stack() {
                            buffer.forget_transaction(entry.transaction_id());
                        }
                    });

                    cx.update(|window, cx| {
                        cx.new(|cx| {
                            let mut editor = Editor::for_buffer(buffer, Some(project), window, cx);

                            editor.read_metadata_from_db(item_id, workspace_id, window, cx);
                            editor
                        })
                    })
                }
            }),
            SerializedEditor {
                abs_path: Some(abs_path),
                contents,
                mtime,
                ..
            } => {
                let opened_buffer = project.update(cx, |project, cx| {
                    let (worktree, path) = project.find_worktree(&abs_path, cx)?;
                    let project_path = ProjectPath {
                        worktree_id: worktree.read(cx).id(),
                        path: path,
                    };
                    Some(project.open_path(project_path, cx))
                });

                match opened_buffer {
                    Some(opened_buffer) => window.spawn(cx, async move |cx| {
                        let (_, buffer) = opened_buffer
                            .await
                            .context("Failed to open path in project")?;

                        if let Some(contents) = contents {
                            buffer.update(cx, |buffer, cx| {
                                restore_serialized_buffer_contents(buffer, contents, mtime, cx);
                            });
                        }

                        cx.update(|window, cx| {
                            cx.new(|cx| {
                                let mut editor =
                                    Editor::for_buffer(buffer, Some(project), window, cx);

                                editor.read_metadata_from_db(item_id, workspace_id, window, cx);
                                editor
                            })
                        })
                    }),
                    None => {
                        // File is not in any worktree (e.g., opened as a standalone file).
                        // Open the buffer directly via the project rather than through
                        // workspace.open_abs_path(), which has the side effect of adding
                        // the item to a pane. The caller (deserialize_to) will add the
                        // returned item to the correct pane.
                        window.spawn(cx, async move |cx| {
                            let buffer = project
                                .update(cx, |project, cx| project.open_local_buffer(&abs_path, cx))
                                .await
                                .with_context(|| {
                                    format!("Failed to open buffer for {abs_path:?}")
                                })?;

                            if let Some(contents) = contents {
                                buffer.update(cx, |buffer, cx| {
                                    restore_serialized_buffer_contents(buffer, contents, mtime, cx);
                                });
                            }

                            cx.update(|window, cx| {
                                cx.new(|cx| {
                                    let mut editor =
                                        Editor::for_buffer(buffer, Some(project), window, cx);
                                    editor.read_metadata_from_db(item_id, workspace_id, window, cx);
                                    editor
                                })
                            })
                        })
                    }
                }
            }
            SerializedEditor {
                abs_path: None,
                contents: None,
                ..
            } => window.spawn(cx, async move |cx| {
                let buffer = project
                    .update(cx, |project, cx| project.create_buffer(None, true, cx))
                    .await
                    .context("Failed to create buffer")?;

                cx.update(|window, cx| {
                    cx.new(|cx| {
                        let mut editor = Editor::for_buffer(buffer, Some(project), window, cx);

                        editor.read_metadata_from_db(item_id, workspace_id, window, cx);
                        editor
                    })
                })
            }),
        }
    }

    fn serialize(
        &mut self,
        workspace: &mut Workspace,
        item_id: ItemId,
        closing: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Task<Result<()>>> {
        let buffer_serialization = self.buffer_serialization?;
        let project = self.project.clone()?;

        let serialize_dirty_buffers = match buffer_serialization {
            // Always serialize dirty buffers, including for worktree-less windows.
            // This enables hot-exit functionality for empty windows and single files.
            BufferSerialization::All => true,
            BufferSerialization::NonDirtyBuffers => false,
        };

        if closing && !serialize_dirty_buffers {
            return None;
        }

        let workspace_id = workspace.database_id()?;

        let buffer = self.buffer().read(cx).as_singleton()?;

        let abs_path = buffer.read(cx).file().and_then(|file| {
            let worktree_id = file.worktree_id(cx);
            project
                .read(cx)
                .worktree_for_id(worktree_id, cx)
                .map(|worktree| worktree.read(cx).absolutize(file.path()))
                .or_else(|| {
                    let full_path = file.full_path(cx);
                    let project_path = project.read(cx).find_project_path(&full_path, cx)?;
                    project.read(cx).absolute_path(&project_path, cx)
                })
        });

        let is_dirty = buffer.read(cx).is_dirty();
        let mtime = buffer.read(cx).saved_mtime();

        let snapshot = buffer.read(cx).snapshot();

        let db = EditorDb::global(cx);
        Some(cx.spawn_in(window, async move |_this, cx| {
            cx.background_spawn(async move {
                let (contents, language) = if serialize_dirty_buffers && is_dirty {
                    let contents = snapshot.text();
                    let language = snapshot.language().map(|lang| lang.name().to_string());
                    (Some(contents), language)
                } else {
                    (None, None)
                };

                let editor = SerializedEditor {
                    abs_path,
                    contents,
                    language,
                    mtime,
                };
                log::debug!("Serializing editor {item_id:?} in workspace {workspace_id:?}");
                db.save_serialized_editor(item_id, workspace_id, editor)
                    .await
                    .context("failed to save serialized editor")
            })
            .await
            .context("failed to save contents of buffer")?;

            Ok(())
        }))
    }

    fn should_serialize(&self, event: &Self::Event) -> bool {
        self.should_serialize_buffer()
            && matches!(
                event,
                EditorEvent::Saved
                    | EditorEvent::DirtyChanged
                    | EditorEvent::BufferEdited
                    | EditorEvent::FileHandleChanged
            )
    }
}
