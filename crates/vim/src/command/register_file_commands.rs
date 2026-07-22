use super::*;

pub(super) fn register_file_commands(editor: &mut Editor, cx: &mut Context<Vim>) {
    Vim::action(editor, cx, |vim, action: &VimSplit, window, cx| {
        let Some(workspace) = vim.workspace(window, cx) else {
            return;
        };

        workspace.update(cx, |workspace, cx| {
            let project = workspace.project().clone();
            let Some(worktree) = project.read(cx).visible_worktrees(cx).next() else {
                return;
            };
            let path_style = worktree.read(cx).path_style();
            let Some(path) = RelPath::new(Path::new(&action.filename), path_style).log_err() else {
                return;
            };
            let project_path = ProjectPath {
                worktree_id: worktree.read(cx).id(),
                path: path.into_arc(),
            };

            let direction = if action.vertical {
                SplitDirection::vertical(cx)
            } else {
                SplitDirection::horizontal(cx)
            };

            workspace
                .split_path_preview(project_path, false, Some(direction), window, cx)
                .detach_and_log_err(cx);
        })
    });

    Vim::action(editor, cx, |vim, action: &DeleteMarks, window, cx| {
        fn err(s: String, window: &mut Window, cx: &mut Context<Editor>) {
            let _ = window.prompt(
                gpui::PromptLevel::Critical,
                &format!("Invalid argument: {}", s),
                None,
                &["Cancel"],
                cx,
            );
        }
        vim.update_editor(cx, |vim, editor, cx| match action {
            DeleteMarks::Marks(s) => {
                if s.starts_with('-') || s.ends_with('-') || s.contains(['\'', '`']) {
                    err(s.clone(), window, cx);
                    return;
                }

                let to_delete = if s.len() < 3 {
                    Some(s.clone())
                } else {
                    s.chars()
                        .tuple_windows::<(_, _, _)>()
                        .map(|(a, b, c)| {
                            if b == '-' {
                                if match a {
                                    'a'..='z' => a <= c && c <= 'z',
                                    'A'..='Z' => a <= c && c <= 'Z',
                                    '0'..='9' => a <= c && c <= '9',
                                    _ => false,
                                } {
                                    Some((a..=c).collect_vec())
                                } else {
                                    None
                                }
                            } else if a == '-' {
                                if c == '-' { None } else { Some(vec![c]) }
                            } else if c == '-' {
                                if a == '-' { None } else { Some(vec![a]) }
                            } else {
                                Some(vec![a, b, c])
                            }
                        })
                        .fold_options(HashSet::<char>::default(), |mut set, chars| {
                            set.extend(chars.iter().copied());
                            set
                        })
                        .map(|set| set.iter().collect::<String>())
                };

                let Some(to_delete) = to_delete else {
                    err(s.clone(), window, cx);
                    return;
                };

                for c in to_delete.chars().filter(|c| !c.is_whitespace()) {
                    vim.delete_mark(c.to_string(), editor, window, cx);
                }
            }
            DeleteMarks::AllLocal => {
                for s in 'a'..='z' {
                    vim.delete_mark(s.to_string(), editor, window, cx);
                }
            }
        });
    });

    Vim::action(editor, cx, |vim, action: &VimEdit, window, cx| {
        vim.update_editor(cx, |vim, editor, cx| {
            let Some(workspace) = vim.workspace(window, cx) else {
                return;
            };
            let Some(project) = editor.project().cloned() else {
                return;
            };
            let Some(worktree) = project.read(cx).visible_worktrees(cx).next() else {
                return;
            };
            let path_style = worktree.read(cx).path_style();
            let Some(path) = RelPath::new(Path::new(&action.filename), path_style).log_err() else {
                return;
            };
            let project_path = ProjectPath {
                worktree_id: worktree.read(cx).id(),
                path: path.into_arc(),
            };

            let _ = workspace.update(cx, |workspace, cx| {
                workspace
                    .open_path(project_path, None, true, window, cx)
                    .detach_and_log_err(cx);
            });
        });
    });

    Vim::action(editor, cx, |vim, action: &VimRead, window, cx| {
        vim.update_editor(cx, |vim, editor, cx| {
            let snapshot = editor.buffer().read(cx).snapshot(cx);
            let end = if let Some(range) = action.range.clone() {
                let Some(multi_range) = range.buffer_range(vim, editor, window, cx).log_err()
                else {
                    return;
                };

                match &range.start {
                    // inserting text above the first line uses the command ":0r {name}"
                    Position::Line { row: 0, offset: 0 } if range.end.is_none() => {
                        snapshot.clip_point(Point::new(0, 0), Bias::Right)
                    }
                    _ => snapshot.clip_point(Point::new(multi_range.end.0 + 1, 0), Bias::Right),
                }
            } else {
                let end_row = editor
                    .selections
                    .newest::<Point>(&editor.display_snapshot(cx))
                    .range()
                    .end
                    .row;
                snapshot.clip_point(Point::new(end_row + 1, 0), Bias::Right)
            };
            let is_end_of_file = end == snapshot.max_point();
            let edit_range = snapshot.anchor_before(end)..snapshot.anchor_before(end);

            let mut text = if is_end_of_file {
                String::from('\n')
            } else {
                String::new()
            };

            let mut task = None;
            if action.filename.is_empty() {
                text.push_str(
                    &editor
                        .buffer()
                        .read(cx)
                        .as_singleton()
                        .map(|buffer| buffer.read(cx).text())
                        .unwrap_or_default(),
                );
            } else {
                if let Some(project) = editor.project().cloned() {
                    project.update(cx, |project, cx| {
                        let Some(worktree) = project.visible_worktrees(cx).next() else {
                            return;
                        };
                        let path_style = worktree.read(cx).path_style();
                        let Some(path) =
                            RelPath::new(Path::new(&action.filename), path_style).log_err()
                        else {
                            return;
                        };
                        task =
                            Some(worktree.update(cx, |worktree, cx| worktree.load_file(&path, cx)));
                    });
                } else {
                    return;
                }
            };

            cx.spawn_in(window, async move |editor, cx| {
                if let Some(task) = task {
                    text.push_str(
                        &task
                            .await
                            .log_err()
                            .map(|loaded_file| loaded_file.text)
                            .unwrap_or_default(),
                    );
                }

                if !text.is_empty() && !is_end_of_file {
                    text.push('\n');
                }

                let _ = editor.update_in(cx, |editor, window, cx| {
                    editor.transact(window, cx, |editor, window, cx| {
                        editor.edit([(edit_range.clone(), text)], cx);
                        let snapshot = editor.buffer().read(cx).snapshot(cx);
                        editor.change_selections(Default::default(), window, cx, |s| {
                            let point = if is_end_of_file {
                                Point::new(
                                    edit_range.start.to_point(&snapshot).row.saturating_add(1),
                                    0,
                                )
                            } else {
                                Point::new(edit_range.start.to_point(&snapshot).row, 0)
                            };
                            s.select_ranges([point..point]);
                        })
                    });
                });
            })
            .detach();
        });
    });
}
