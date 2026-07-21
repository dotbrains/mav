use super::*;

pub(super) fn insert_mention_for_project_path(
    project_path: &ProjectPath,
    editor: &Entity<Editor>,
    mention_set: &Entity<MentionSet>,
    project: &Entity<Project>,
    workspace: &Entity<Workspace>,
    supports_images: bool,
    window: &mut Window,
    cx: &mut App,
) -> Option<Task<()>> {
    let (file_name, mention_uri) = {
        let project = project.read(cx);
        let path_style = project.path_style(cx);
        let entry = project.entry_for_path(project_path, cx)?;
        let worktree = project.worktree_for_id(project_path.worktree_id, cx)?;
        let abs_path = worktree.read(cx).absolutize(&project_path.path);
        let (file_name, _) = crate::completion_provider::extract_file_name_and_directory(
            &project_path.path,
            worktree.read(cx).root_name(),
            path_style,
        );
        let mention_uri = if entry.is_dir() {
            MentionUri::Directory { abs_path }
        } else {
            MentionUri::File { abs_path }
        };
        (file_name, mention_uri)
    };

    let mention_text = mention_uri.as_link().to_string();
    let content_len = mention_text.len();

    let text_anchor = editor.update(cx, |editor, cx| {
        let buffer = editor.buffer().read(cx);
        let snapshot = buffer.snapshot(cx);
        let buffer_snapshot = snapshot.as_singleton()?;
        let text_anchor = snapshot
            .anchor_to_buffer_anchor(editor.selections.newest_anchor().start)?
            .0
            .bias_left(&buffer_snapshot);

        editor.insert(&mention_text, window, cx);
        editor.insert(" ", window, cx);

        Some(text_anchor)
    })?;

    Some(mention_set.update(cx, |mention_set, cx| {
        mention_set.confirm_mention_completion(
            file_name,
            text_anchor,
            content_len,
            mention_uri,
            supports_images,
            editor.clone(),
            workspace,
            window,
            cx,
        )
    }))
}

pub(super) enum ResolvedPastedContextItem {
    Image(gpui::Image, gpui::SharedString),
    ProjectPath(ProjectPath),
}

pub(super) async fn resolve_pasted_context_items(
    project: Entity<Project>,
    project_is_local: bool,
    supports_images: bool,
    entries: Vec<ClipboardEntry>,
    cx: &mut gpui::AsyncWindowContext,
) -> (Vec<ResolvedPastedContextItem>, Vec<Entity<Worktree>>) {
    let mut items = Vec::new();
    let mut added_worktrees = Vec::new();
    let default_image_name: SharedString = "Image".into();

    for entry in entries {
        match entry {
            ClipboardEntry::String(_) => {}
            ClipboardEntry::Image(image) => {
                if supports_images {
                    items.push(ResolvedPastedContextItem::Image(
                        image,
                        default_image_name.clone(),
                    ));
                }
            }
            ClipboardEntry::ExternalPaths(paths) => {
                for path in paths.paths().iter() {
                    if let Some((image, name)) = cx
                        .background_spawn({
                            let path = path.clone();
                            let default_image_name = default_image_name.clone();
                            async move {
                                crate::mention_set::load_external_image_from_path(
                                    &path,
                                    &default_image_name,
                                )
                            }
                        })
                        .await
                    {
                        if supports_images {
                            items.push(ResolvedPastedContextItem::Image(image, name));
                        }
                        continue;
                    }

                    if !project_is_local {
                        continue;
                    }

                    let path = path.clone();
                    let Ok(resolve_task) = cx.update({
                        let project = project.clone();
                        move |_, cx| Workspace::project_path_for_path(project, &path, false, cx)
                    }) else {
                        continue;
                    };

                    if let Some((worktree, project_path)) = resolve_task.await.log_err() {
                        added_worktrees.push(worktree);
                        items.push(ResolvedPastedContextItem::ProjectPath(project_path));
                    }
                }
            }
        }
    }

    (items, added_worktrees)
}

fn insert_project_path_as_context(
    project_path: ProjectPath,
    editor: Entity<Editor>,
    mention_set: Entity<MentionSet>,
    workspace: WeakEntity<Workspace>,
    supports_images: bool,
    cx: &mut gpui::AsyncWindowContext,
) -> Option<Task<()>> {
    let workspace = workspace.upgrade()?;

    cx.update(move |window, cx| {
        let project = workspace.read(cx).project().clone();
        insert_mention_for_project_path(
            &project_path,
            &editor,
            &mention_set,
            &project,
            &workspace,
            supports_images,
            window,
            cx,
        )
    })
    .ok()
    .flatten()
}

pub(super) async fn insert_resolved_pasted_context_items(
    items: Vec<ResolvedPastedContextItem>,
    added_worktrees: Vec<Entity<Worktree>>,
    editor: Entity<Editor>,
    mention_set: Entity<MentionSet>,
    workspace: WeakEntity<Workspace>,
    supports_images: bool,
    cx: &mut gpui::AsyncWindowContext,
) {
    let mut path_mention_tasks = Vec::new();

    for item in items {
        match item {
            ResolvedPastedContextItem::Image(image, name) => {
                crate::mention_set::insert_images_as_context(
                    vec![(image, name)],
                    editor.clone(),
                    mention_set.clone(),
                    workspace.clone(),
                    cx,
                )
                .await;
            }
            ResolvedPastedContextItem::ProjectPath(project_path) => {
                if let Some(task) = insert_project_path_as_context(
                    project_path,
                    editor.clone(),
                    mention_set.clone(),
                    workspace.clone(),
                    supports_images,
                    cx,
                ) {
                    path_mention_tasks.push(task);
                }
            }
        }
    }

    join_all(path_mention_tasks).await;
    drop(added_worktrees);
}
