use super::*;

pub(super) fn register_save(editor: &mut Editor, cx: &mut Context<Vim>) {
    Vim::action(editor, cx, |vim, action: &VimSave, window, cx| {
        if let Some(range) = &action.range {
            vim.update_editor(cx, |vim, editor, cx| {
                let Some(range) = range.buffer_range(vim, editor, window, cx).ok() else {
                    return;
                };
                let Some((line_ending, encoding, has_bom, text, whole_buffer)) = editor.buffer().update(cx, |multi, cx| {
                    Some(multi.as_singleton()?.update(cx, |buffer, _| {
                        (
                            buffer.line_ending(),
                            buffer.encoding(),
                            buffer.has_bom(),
                            buffer.as_rope().slice_rows(range.start.0..range.end.0 + 1),
                            range.start.0 == 0 && range.end.0 + 1 >= buffer.row_count(),
                        )
                    }))
                }) else {
                    return;
                };

                let filename = action.filename.clone();
                let filename = if filename.is_empty() {
                    let Some(file) = editor
                        .buffer()
                        .read(cx)
                        .as_singleton()
                        .and_then(|buffer| buffer.read(cx).file())
                    else {
                        let _ = window.prompt(
                            gpui::PromptLevel::Warning,
                            "No file name",
                            Some("Partial buffer write requires file name."),
                            &["Cancel"],
                            cx,
                        );
                        return;
                    };
                    file.path().display(file.path_style(cx)).to_string()
                } else {
                    filename
                };

                if action.filename.is_empty() {
                    if whole_buffer {
                        if let Some(workspace) = vim.workspace(window, cx) {
                            workspace.update(cx, |workspace, cx| {
                                workspace
                                    .save_active_item(
                                        action.save_intent.unwrap_or(SaveIntent::Save),
                                        window,
                                        cx,
                                    )
                                    .detach_and_prompt_err("Failed to save", window, cx, |_, _, _| None);
                            });
                        }
                        return;
                    }
                    if Some(SaveIntent::Overwrite) != action.save_intent {
                        let _ = window.prompt(
                            gpui::PromptLevel::Warning,
                            "Use ! to write partial buffer",
                            Some("Overwriting the current file with selected buffer content requires '!'."),
                            &["Cancel"],
                            cx,
                        );
                        return;
                    }
                    editor.buffer().update(cx, |multi, cx| {
                        if let Some(buffer) = multi.as_singleton() {
                            buffer.update(cx, |buffer, _| buffer.set_conflict());
                        }
                    });
                };

                editor.project().unwrap().update(cx, |project, cx| {
                    let worktree = project.visible_worktrees(cx).next().unwrap();

                    worktree.update(cx, |worktree, cx| {
                        let path_style = worktree.path_style();
                        let Some(path) = RelPath::new(Path::new(&filename), path_style).ok() else {
                            return;
                        };

                        let rx = (worktree.entry_for_path(&path).is_some() && Some(SaveIntent::Overwrite) != action.save_intent).then(|| {
                            window.prompt(
                                gpui::PromptLevel::Warning,
                                &format!("{path:?} already exists. Do you want to replace it?"),
                                Some(
                                    "A file or folder with the same name already exists. Replacing it will overwrite its current contents.",
                                ),
                                &["Replace", "Cancel"],
                                cx
                            )
                        });
                        let filename = filename.clone();
                        cx.spawn_in(window, async move |this, cx| {
                            if let Some(rx) = rx
                                && Ok(0) != rx.await
                            {
                                return;
                            }

                            let _ = this.update_in(cx, |worktree, window, cx| {
                                let Some(path) = RelPath::new(Path::new(&filename), path_style).ok() else {
                                    return;
                                };
                                worktree
                                    .write_file(path.into_arc(), text.clone(), line_ending, encoding, has_bom, cx)
                                    .detach_and_prompt_err("Failed to write lines", window, cx, |_, _, _| None);
                            });
                        })
                        .detach();
                    });
                });
            });
            return;
        }
        if action.filename.is_empty() {
            if let Some(workspace) = vim.workspace(window, cx) {
                workspace.update(cx, |workspace, cx| {
                    workspace
                        .save_active_item(
                            action.save_intent.unwrap_or(SaveIntent::Save),
                            window,
                            cx,
                        )
                        .detach_and_prompt_err("Failed to save", window, cx, |_, _, _| None);
                });
            }
            return;
        }
        vim.update_editor(cx, |_, editor, cx| {
            let Some(project) = editor.project().cloned() else {
                return;
            };
            let Some(worktree) = project.read(cx).visible_worktrees(cx).next() else {
                return;
            };
            let path_style = worktree.read(cx).path_style();
            let Ok(project_path) =
                RelPath::new(Path::new(&action.filename), path_style).map(|path| ProjectPath {
                    worktree_id: worktree.read(cx).id(),
                    path: path.into_arc(),
                })
            else {
                // TODO implement save_as with absolute path
                Task::ready(Err::<(), _>(anyhow!(
                    "Cannot save buffer with absolute path"
                )))
                .detach_and_prompt_err(
                    "Failed to save",
                    window,
                    cx,
                    |_, _, _| None,
                );
                return;
            };

            if project.read(cx).entry_for_path(&project_path, cx).is_some()
                && action.save_intent != Some(SaveIntent::Overwrite)
            {
                let answer = window.prompt(
                    gpui::PromptLevel::Critical,
                    &format!(
                        "{} already exists. Do you want to replace it?",
                        project_path.path.display(path_style)
                    ),
                    Some(
                        "A file or folder with the same name already exists. \
                        Replacing it will overwrite its current contents.",
                    ),
                    &["Replace", "Cancel"],
                    cx,
                );
                cx.spawn_in(window, async move |editor, cx| {
                    if answer.await.ok() != Some(0) {
                        return;
                    }

                    let _ = editor.update_in(cx, |editor, window, cx| {
                        editor
                            .save_as(project, project_path, window, cx)
                            .detach_and_prompt_err("Failed to :w", window, cx, |_, _, _| None);
                    });
                })
                .detach();
            } else {
                editor
                    .save_as(project, project_path, window, cx)
                    .detach_and_prompt_err("Failed to :w", window, cx, |_, _, _| None);
            }
        });
    });
}
