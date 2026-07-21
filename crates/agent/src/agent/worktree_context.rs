use super::*;

impl NativeAgent {
    pub(super) fn load_worktree_info_for_system_prompt(
        worktree: Entity<Worktree>,
        project: Entity<Project>,
        cx: &mut App,
    ) -> Task<(WorktreeContext, Option<RulesLoadingError>)> {
        let tree = worktree.read(cx);
        let root_name = tree.root_name_str().into();
        let abs_path = tree.abs_path();
        let scan_complete = tree.as_local().map(|local| local.scan_complete());

        let mut context = WorktreeContext {
            root_name,
            abs_path,
            rules_file: None,
        };

        cx.spawn(async move |cx| {
            if let Some(scan_complete) = scan_complete {
                scan_complete.await;
            }

            let rules_task = cx.update(|cx| Self::load_worktree_rules_file(worktree, project, cx));

            let (rules_file, rules_file_error) = match rules_task {
                Some(rules_task) => match rules_task.await {
                    Ok(rules_file) => (Some(rules_file), None),
                    Err(err) => (
                        None,
                        Some(RulesLoadingError {
                            message: format!("{err}").into(),
                        }),
                    ),
                },
                None => (None, None),
            };
            context.rules_file = rules_file;
            (context, rules_file_error)
        })
    }

    fn load_worktree_rules_file(
        worktree: Entity<Worktree>,
        project: Entity<Project>,
        cx: &mut App,
    ) -> Option<Task<Result<RulesFileContext>>> {
        let worktree = worktree.read(cx);
        let worktree_id = worktree.id();
        let selected_rules_file = RULES_FILE_REL_PATHS
            .iter()
            .filter_map(|name| {
                worktree
                    .entry_for_path(name)
                    .filter(|entry| entry.is_file())
                    .map(|entry| entry.path.clone())
            })
            .next();

        // Note that Cline supports `.clinerules` being a directory, but that is not currently
        // supported. This doesn't seem to occur often in GitHub repositories.
        selected_rules_file.map(|path_in_worktree| {
            let project_path = ProjectPath {
                worktree_id,
                path: path_in_worktree.clone(),
            };
            let buffer_task =
                project.update(cx, |project, cx| project.open_buffer(project_path, cx));
            let rope_task = cx.spawn(async move |cx| {
                let buffer = buffer_task.await?;
                let (project_entry_id, rope) = buffer.read_with(cx, |buffer, cx| {
                    let project_entry_id = buffer.entry_id(cx).context("buffer has no file")?;
                    anyhow::Ok((project_entry_id, buffer.as_rope().clone()))
                })?;
                anyhow::Ok((project_entry_id, rope))
            });
            // Build a string from the rope on a background thread.
            cx.background_spawn(async move {
                let (project_entry_id, rope) = rope_task.await?;
                anyhow::Ok(RulesFileContext {
                    path_in_worktree,
                    text: rope.to_string().trim().to_string(),
                    project_entry_id: project_entry_id.to_usize(),
                })
            })
        })
    }
}
