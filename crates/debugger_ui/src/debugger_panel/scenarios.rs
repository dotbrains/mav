use super::*;

impl DebugPanel {
    pub(crate) fn go_to_scenario_definition(
        &self,
        kind: TaskSourceKind,
        scenario: DebugScenario,
        worktree_id: WorktreeId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        let Some(workspace) = self.workspace.upgrade() else {
            return Task::ready(Ok(()));
        };
        let project_path = match kind {
            TaskSourceKind::AbsPath { abs_path, .. } => {
                let Some(project_path) = workspace
                    .read(cx)
                    .project()
                    .read(cx)
                    .project_path_for_absolute_path(&abs_path, cx)
                else {
                    return Task::ready(Err(anyhow!("no abs path")));
                };

                project_path
            }
            TaskSourceKind::Worktree {
                id,
                directory_in_worktree: dir,
                ..
            } => {
                let relative_path = if dir.ends_with(RelPath::unix(".vscode").unwrap()) {
                    dir.join(RelPath::unix("launch.json").unwrap())
                } else {
                    dir.join(RelPath::unix("debug.json").unwrap())
                };
                ProjectPath {
                    worktree_id: id,
                    path: relative_path,
                }
            }
            _ => return self.save_scenario(scenario, worktree_id, window, cx),
        };

        let editor = workspace.update(cx, |workspace, cx| {
            workspace.open_path(project_path, None, true, window, cx)
        });
        cx.spawn_in(window, async move |_, cx| {
            let editor = editor.await?;
            let editor = cx
                .update(|_, cx| editor.act_as::<Editor>(cx))?
                .context("expected editor")?;

            // unfortunately debug tasks don't have an easy way to globally
            // identify them. to jump to the one that you just created or an
            // old one that you're choosing to edit we use a heuristic of searching for a line with `label:  <your label>` from the end rather than the start so we bias towards more renctly
            editor.update_in(cx, |editor, window, cx| {
                let row = editor.text(cx).lines().enumerate().find_map(|(row, text)| {
                    if text.contains(scenario.label.as_ref()) && text.contains("\"label\": ") {
                        Some(row)
                    } else {
                        None
                    }
                });
                if let Some(row) = row {
                    editor.go_to_singleton_buffer_point(
                        text::Point::new(row as u32, 4),
                        window,
                        cx,
                    );
                }
            })?;

            Ok(())
        })
    }

    pub(crate) fn save_scenario(
        &self,
        scenario: DebugScenario,
        worktree_id: WorktreeId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        let this = cx.weak_entity();
        let project = self.project.clone();
        self.workspace
            .update(cx, |workspace, cx| {
                let Some(mut path) = workspace.absolute_path_of_worktree(worktree_id, cx) else {
                    return Task::ready(Err(anyhow!("Couldn't get worktree path")));
                };

                let serialized_scenario = serde_json::to_value(scenario);

                cx.spawn_in(window, async move |workspace, cx| {
                    let serialized_scenario = serialized_scenario?;
                    let fs =
                        workspace.read_with(cx, |workspace, _| workspace.app_state().fs.clone())?;

                    path.push(paths::local_settings_folder_name());
                    if !fs.is_dir(path.as_path()).await {
                        fs.create_dir(path.as_path()).await?;
                    }
                    path.pop();

                    path.push(paths::local_debug_file_relative_path().as_std_path());
                    let path = path.as_path();

                    if !fs.is_file(path).await {
                        fs.create_file(path, Default::default()).await?;
                        fs.write(
                            path,
                            settings::initial_local_debug_tasks_content()
                                .to_string()
                                .as_bytes(),
                        )
                        .await?;
                    }
                    let project_path = workspace.update(cx, |workspace, cx| {
                        workspace
                            .project()
                            .read(cx)
                            .project_path_for_absolute_path(path, cx)
                            .context(
                                "Couldn't get project path for .mav/debug.json in active worktree",
                            )
                    })??;

                    let editor = this
                        .update_in(cx, |this, window, cx| {
                            this.workspace.update(cx, |workspace, cx| {
                                workspace.open_path(project_path, None, true, window, cx)
                            })
                        })??
                        .await?;
                    let editor = cx
                        .update(|_, cx| editor.act_as::<Editor>(cx))?
                        .context("expected editor")?;

                    let new_scenario = serde_json_lenient::to_string_pretty(&serialized_scenario)?
                        .lines()
                        .map(|l| format!("  {l}"))
                        .join("\n");

                    editor
                        .update_in(cx, |editor, window, cx| {
                            Self::insert_task_into_editor(editor, new_scenario, project, window, cx)
                        })??
                        .await
                })
            })
            .unwrap_or_else(|err| Task::ready(Err(err)))
    }

    pub fn insert_task_into_editor(
        editor: &mut Editor,
        new_scenario: String,
        project: Entity<Project>,
        window: &mut Window,
        cx: &mut Context<Editor>,
    ) -> Result<Task<Result<()>>> {
        static LAST_ITEM_QUERY: LazyLock<Query> = LazyLock::new(|| {
            Query::new(
                &tree_sitter_json::LANGUAGE.into(),
                "(document (array (object) @object))", // TODO: use "." anchor to only match last object
            )
            .expect("Failed to create LAST_ITEM_QUERY")
        });
        static EMPTY_ARRAY_QUERY: LazyLock<Query> = LazyLock::new(|| {
            Query::new(
                &tree_sitter_json::LANGUAGE.into(),
                "(document (array) @array)",
            )
            .expect("Failed to create EMPTY_ARRAY_QUERY")
        });

        let content = editor.text(cx);
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_json::LANGUAGE.into())?;
        let mut cursor = tree_sitter::QueryCursor::new();
        let syntax_tree = parser
            .parse(&content, None)
            .context("could not parse debug.json")?;
        let mut matches = cursor.matches(
            &LAST_ITEM_QUERY,
            syntax_tree.root_node(),
            content.as_bytes(),
        );

        let mut last_offset = None;
        while let Some(mat) = matches.next() {
            if let Some(pos) = mat.captures.first().map(|m| m.node.byte_range().end) {
                last_offset = Some(MultiBufferOffset(pos))
            }
        }
        let mut edits = Vec::new();
        let mut cursor_position = MultiBufferOffset(0);

        if let Some(pos) = last_offset {
            edits.push((pos..pos, format!(",\n{new_scenario}")));
            cursor_position = pos + ",\n  ".len();
        } else {
            let mut matches = cursor.matches(
                &EMPTY_ARRAY_QUERY,
                syntax_tree.root_node(),
                content.as_bytes(),
            );

            if let Some(mat) = matches.next() {
                if let Some(pos) = mat.captures.first().map(|m| m.node.byte_range().end - 1) {
                    edits.push((
                        MultiBufferOffset(pos)..MultiBufferOffset(pos),
                        format!("\n{new_scenario}\n"),
                    ));
                    cursor_position = MultiBufferOffset(pos) + "\n  ".len();
                }
            } else {
                edits.push((
                    MultiBufferOffset(0)..MultiBufferOffset(0),
                    format!("[\n{}\n]", new_scenario),
                ));
                cursor_position = MultiBufferOffset("[\n  ".len());
            }
        }
        editor.transact(window, cx, |editor, window, cx| {
            editor.edit(edits, cx);
            let snapshot = editor.buffer().read(cx).read(cx);
            let point = cursor_position.to_point(&snapshot);
            drop(snapshot);
            editor.go_to_singleton_buffer_point(point, window, cx);
        });
        Ok(editor.save(SaveOptions::default(), project, window, cx))
    }
}
