use super::*;

pub(super) fn dispatch_apply_templates(
    template_entry: TemplateEntry,
    workspace: Entity<Workspace>,
    window: &mut Window,
    check_for_existing: bool,
    cx: &mut Context<DevContainerModal>,
) {
    cx.spawn_in(window, async move |this, cx| {
        let Some((tree_id, context)) = workspace.update(cx, |workspace, cx| {
            let worktree = workspace
                .project()
                .read(cx)
                .visible_worktrees(cx)
                .find_map(|tree| {
                    tree.read(cx)
                        .root_entry()?
                        .is_dir()
                        .then_some(tree.read(cx))
                });
            let tree_id = worktree.map(|w| w.id())?;
            let context = DevContainerContext::from_workspace(workspace, cx)?;
            Some((tree_id, context))
        }) else {
            return;
        };

        let environment = context.environment(cx).await;

        {
            if check_for_existing
                && read_default_devcontainer_configuration(&context, environment)
                    .await
                    .is_ok()
            {
                this.update_in(cx, |this, window, cx| {
                    this.accept_message(
                        DevContainerMessage::NeedConfirmWriteDevContainer(template_entry),
                        window,
                        cx,
                    );
                })
                .ok();
                return;
            }

            let worktree = workspace.read_with(cx, |workspace, cx| {
                workspace.project().read(cx).worktree_for_id(tree_id, cx)
            });

            let files = match apply_devcontainer_template(
                worktree.unwrap(),
                &template_entry.template,
                &template_entry.options_selected,
                &template_entry.features_selected,
                &context,
                cx,
            )
            .await
            {
                Ok(files) => files,
                Err(e) => {
                    this.update_in(cx, |this, window, cx| {
                        this.accept_message(
                            DevContainerMessage::FailedToWriteTemplate(
                                DevContainerError::DevContainerTemplateApplyFailed(e.to_string()),
                            ),
                            window,
                            cx,
                        );
                    })
                    .ok();
                    return;
                }
            };

            if files.project_files.contains(&Arc::from(
                RelPath::unix(".devcontainer/devcontainer.json").unwrap(),
            )) {
                let Some(workspace_task) = workspace
                    .update_in(cx, |workspace, window, cx| {
                        let Ok(path) = RelPath::unix(".devcontainer/devcontainer.json") else {
                            return Task::ready(Err(anyhow!(
                                "Couldn't create path for .devcontainer/devcontainer.json"
                            )));
                        };
                        workspace.open_path((tree_id, path), None, true, window, cx)
                    })
                    .ok()
                else {
                    return;
                };

                workspace_task.await.log_err();
            }
            this.update_in(cx, |this, window, cx| {
                this.dismiss(&menu::Cancel, window, cx);
            })
            .ok();
        }
    })
    .detach();
}
