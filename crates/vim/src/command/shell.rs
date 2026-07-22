use super::*;

/// Executes a shell command and returns the output.
#[derive(Clone, Debug, PartialEq, Action)]
#[action(namespace = vim, no_json, no_register)]
pub struct ShellExec {
    command: String,
    range: Option<CommandRange>,
    is_read: bool,
}

impl Vim {
    pub fn cancel_running_command(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.running_command.take().is_some() {
            self.update_editor(cx, |_, editor, cx| {
                editor.transact(window, cx, |editor, _window, _cx| {
                    editor.clear_row_highlights::<ShellExec>();
                })
            });
        }
    }

    fn prepare_shell_command(
        &mut self,
        command: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> String {
        let mut ret = String::new();
        // N.B. non-standard escaping rules:
        // * !echo % => "echo README.md"
        // * !echo \% => "echo %"
        // * !echo \\% => echo \%
        // * !echo \\\% => echo \\%
        for c in command.chars() {
            if c != '%' && c != '!' {
                ret.push(c);
                continue;
            } else if ret.chars().last() == Some('\\') {
                ret.pop();
                ret.push(c);
                continue;
            }
            match c {
                '%' => {
                    self.update_editor(cx, |_, editor, cx| {
                        if let Some(buffer) = editor.active_buffer(cx)
                            && let Some(file) = buffer.read(cx).file()
                            && let Some(local) = file.as_local()
                        {
                            ret.push_str(&local.path().display(local.path_style(cx)));
                        }
                    });
                }
                '!' => {
                    if let Some(command) = &self.last_command {
                        ret.push_str(command)
                    }
                }
                _ => {}
            }
        }
        self.last_command = Some(ret.clone());
        ret
    }

    pub fn shell_command_motion(
        &mut self,
        motion: Motion,
        times: Option<usize>,
        forced_motion: bool,
        window: &mut Window,
        cx: &mut Context<Vim>,
    ) {
        self.stop_recording(cx);
        let Some(workspace) = self.workspace(window, cx) else {
            return;
        };
        let command = self.update_editor(cx, |_, editor, cx| {
            let snapshot = editor.snapshot(window, cx);
            let start = editor
                .selections
                .newest_display(&editor.display_snapshot(cx));
            let text_layout_details = editor.text_layout_details(window, cx);
            let (mut range, _) = motion
                .range(
                    &snapshot,
                    start.clone(),
                    times,
                    &text_layout_details,
                    forced_motion,
                )
                .unwrap_or((start.range(), MotionKind::Exclusive));
            if range.start != start.start {
                editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
                    s.select_ranges([
                        range.start.to_point(&snapshot)..range.start.to_point(&snapshot)
                    ]);
                })
            }
            if range.end.row() > range.start.row() && range.end.column() != 0 {
                *range.end.row_mut() -= 1
            }
            if range.end.row() == range.start.row() {
                ".!".to_string()
            } else {
                format!(".,.+{}!", (range.end.row() - range.start.row()).0)
            }
        });
        if let Some(command) = command {
            workspace.update(cx, |workspace, cx| {
                command_palette::CommandPalette::toggle(workspace, &command, window, cx);
            });
        }
    }

    pub fn shell_command_object(
        &mut self,
        object: Object,
        around: bool,
        window: &mut Window,
        cx: &mut Context<Vim>,
    ) {
        self.stop_recording(cx);
        let Some(workspace) = self.workspace(window, cx) else {
            return;
        };
        let command = self.update_editor(cx, |_, editor, cx| {
            let snapshot = editor.snapshot(window, cx);
            let start = editor
                .selections
                .newest_display(&editor.display_snapshot(cx));
            let range = object
                .range(&snapshot, start.clone(), around, None)
                .unwrap_or(start.range());
            if range.start != start.start {
                editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
                    s.select_ranges([
                        range.start.to_point(&snapshot)..range.start.to_point(&snapshot)
                    ]);
                })
            }
            if range.end.row() == range.start.row() {
                ".!".to_string()
            } else {
                format!(".,.+{}!", (range.end.row() - range.start.row()).0)
            }
        });
        if let Some(command) = command {
            workspace.update(cx, |workspace, cx| {
                command_palette::CommandPalette::toggle(workspace, &command, window, cx);
            });
        }
    }
}

impl ShellExec {
    pub fn parse(query: &str, range: Option<CommandRange>) -> Option<Box<dyn Action>> {
        let (before, after) = query.split_once('!')?;
        let before = before.trim();

        if !"read".starts_with(before) {
            return None;
        }

        Some(
            ShellExec {
                command: after.trim().to_string(),
                range,
                is_read: !before.is_empty(),
            }
            .boxed_clone(),
        )
    }

    pub fn run(&self, vim: &mut Vim, window: &mut Window, cx: &mut Context<Vim>) {
        let Some(workspace) = vim.workspace(window, cx) else {
            return;
        };

        let project = workspace.read(cx).project().clone();
        let command = vim.prepare_shell_command(&self.command, window, cx);

        if self.range.is_none() && !self.is_read {
            workspace.update(cx, |workspace, cx| {
                let project = workspace.project().read(cx);
                let cwd = project.first_project_directory(cx);
                let shell = Shell::System;

                let spawn_in_terminal = SpawnInTerminal {
                    id: TaskId("vim".to_string()),
                    full_label: command.clone(),
                    label: command.clone(),
                    command: Some(command.clone()),
                    args: Vec::new(),
                    command_label: command.clone(),
                    cwd,
                    env: HashMap::default(),
                    use_new_terminal: true,
                    allow_concurrent_runs: true,
                    reveal: RevealStrategy::NoFocus,
                    reveal_target: RevealTarget::Dock,
                    hide: HideStrategy::Never,
                    shell,
                    show_summary: false,
                    show_command: false,
                    show_rerun: false,
                    save: SaveStrategy::default(),
                };

                let task_status = workspace.spawn_in_terminal(spawn_in_terminal, window, cx);
                cx.background_spawn(async move {
                    match task_status.await {
                        Some(Ok(status)) => {
                            if status.success() {
                                log::debug!("Vim shell exec succeeded");
                            } else {
                                log::debug!("Vim shell exec failed, code: {:?}", status.code());
                            }
                        }
                        Some(Err(e)) => log::error!("Vim shell exec failed: {e}"),
                        None => log::debug!("Vim shell exec got cancelled"),
                    }
                })
                .detach();
            });
            return;
        };

        let mut input_snapshot = None;
        let mut input_range = None;
        let mut needs_newline_prefix = false;
        vim.update_editor(cx, |vim, editor, cx| {
            let snapshot = editor.buffer().read(cx).snapshot(cx);
            let range = if let Some(range) = self.range.clone() {
                let Some(range) = range.buffer_range(vim, editor, window, cx).log_err() else {
                    return;
                };
                Point::new(range.start.0, 0)
                    ..snapshot.clip_point(Point::new(range.end.0 + 1, 0), Bias::Right)
            } else {
                let mut end = editor
                    .selections
                    .newest::<Point>(&editor.display_snapshot(cx))
                    .range()
                    .end;
                end = snapshot.clip_point(Point::new(end.row + 1, 0), Bias::Right);
                needs_newline_prefix = end == snapshot.max_point();
                end..end
            };
            if self.is_read {
                input_range =
                    Some(snapshot.anchor_after(range.end)..snapshot.anchor_after(range.end));
            } else {
                input_range =
                    Some(snapshot.anchor_before(range.start)..snapshot.anchor_after(range.end));
            }
            editor.highlight_rows::<ShellExec>(
                input_range.clone().unwrap(),
                |cx| cx.theme().status().unreachable_background,
                Default::default(),
                cx,
            );

            if !self.is_read {
                input_snapshot = Some(snapshot)
            }
        });

        let Some(range) = input_range else { return };

        let process_task = project.update(cx, |project, cx| project.exec_in_shell(command, cx));

        let is_read = self.is_read;

        let task = cx.spawn_in(window, async move |vim, cx| {
            let Some(mut process) = process_task.await.log_err() else {
                return;
            };
            process.stdout(Stdio::piped());
            process.stderr(Stdio::piped());

            if input_snapshot.is_some() {
                process.stdin(Stdio::piped());
            } else {
                process.stdin(Stdio::null());
            };

            let Some(mut running) = process.spawn().log_err() else {
                vim.update_in(cx, |vim, window, cx| {
                    vim.cancel_running_command(window, cx);
                })
                .log_err();
                return;
            };

            if let Some(mut stdin) = running.stdin.take()
                && let Some(snapshot) = input_snapshot
            {
                let range = range.clone();
                cx.background_spawn(async move {
                    for chunk in snapshot.text_for_range(range) {
                        if stdin.write_all(chunk.as_bytes()).await.log_err().is_none() {
                            return;
                        }
                    }
                    stdin.flush().await.log_err();
                })
                .detach();
            };

            let output = cx.background_spawn(running.output()).await;

            let Some(output) = output.log_err() else {
                vim.update_in(cx, |vim, window, cx| {
                    vim.cancel_running_command(window, cx);
                })
                .log_err();
                return;
            };
            let mut text = String::new();
            if needs_newline_prefix {
                text.push('\n');
            }
            text.push_str(&String::from_utf8_lossy(&output.stdout));
            text.push_str(&String::from_utf8_lossy(&output.stderr));
            if !text.is_empty() && text.chars().last() != Some('\n') {
                text.push('\n');
            }

            vim.update_in(cx, |vim, window, cx| {
                vim.update_editor(cx, |_, editor, cx| {
                    editor.transact(window, cx, |editor, window, cx| {
                        editor.edit([(range.clone(), text)], cx);
                        let snapshot = editor.buffer().read(cx).snapshot(cx);
                        editor.change_selections(Default::default(), window, cx, |s| {
                            let point = if is_read {
                                let point = range.end.to_point(&snapshot);
                                Point::new(point.row.saturating_sub(1), 0)
                            } else {
                                let point = range.start.to_point(&snapshot);
                                Point::new(point.row, 0)
                            };
                            s.select_ranges([point..point]);
                        })
                    })
                });
                vim.cancel_running_command(window, cx);
            })
            .log_err();
        });
        vim.running_command.replace(task);
    }
}
