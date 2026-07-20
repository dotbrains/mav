use super::*;

pub(crate) fn open_link(
    url: SharedString,
    workspace: &WeakEntity<Workspace>,
    window: &mut Window,
    cx: &mut App,
) {
    let Some(workspace) = workspace.upgrade() else {
        cx.open_url(&url);
        return;
    };

    if let Some(mention) = MentionUri::parse(&url, workspace.read(cx).path_style(cx)).log_err() {
        workspace.update(cx, |workspace, cx| match mention {
            MentionUri::File { abs_path } => {
                let project = workspace.project();
                let Some(path) =
                    project.update(cx, |project, cx| project.find_project_path(abs_path, cx))
                else {
                    return;
                };

                workspace
                    .open_path(path, None, true, window, cx)
                    .detach_and_log_err(cx);
            }
            MentionUri::PastedImage { .. } => {}
            MentionUri::Directory { abs_path } => {
                let project = workspace.project();
                let Some(entry_id) = project.update(cx, |project, cx| {
                    let path = project.find_project_path(abs_path, cx)?;
                    project.entry_for_path(&path, cx).map(|entry| entry.id)
                }) else {
                    return;
                };

                project.update(cx, |_, cx| {
                    cx.emit(project::Event::RevealInProjectPanel(entry_id));
                });
            }
            MentionUri::Symbol {
                abs_path: path,
                line_range,
                ..
            } => {
                open_abs_path_at_point(
                    workspace,
                    path,
                    Point::new(*line_range.start(), 0),
                    window,
                    cx,
                );
            }
            MentionUri::Selection {
                abs_path: Some(path),
                line_range,
                column,
            } => {
                open_abs_path_at_point(
                    workspace,
                    path,
                    Point::new(*line_range.start(), column.unwrap_or(0)),
                    window,
                    cx,
                );
            }
            MentionUri::Selection { abs_path: None, .. } => {}
            MentionUri::Thread { id, name } => {
                if let Some(panel) = workspace.panel::<AgentPanel>(cx) {
                    panel.update(cx, |panel, cx| {
                        panel.open_thread(id, None, Some(name.into()), window, cx)
                    });
                }
            }
            MentionUri::Fetch { url } => {
                cx.open_url(url.as_str());
            }
            MentionUri::Diagnostics { .. } => {}
            MentionUri::TerminalSelection { .. } => {}
            MentionUri::GitDiff { .. } => {}
            MentionUri::MergeConflict { .. } => {}
            MentionUri::Rule { name, .. } => {
                crate::ui::open_migrated_rule(workspace, &name, window, cx);
            }
            MentionUri::Skill {
                skill_file_path, ..
            } => {
                workspace
                    .open_abs_path(
                        skill_file_path,
                        workspace::OpenOptions {
                            focus: Some(true),
                            ..Default::default()
                        },
                        window,
                        cx,
                    )
                    .detach_and_log_err(cx);
            }
        })
    } else {
        workspace.update(cx, |workspace, cx| {
            workspace.open_url_or_file(&url, None, window, cx);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use project::{FakeFs, Project};
    use serde_json::json;
    use util::path;
    use workspace::MultiWorkspace;

    #[gpui::test]
    async fn test_open_link_bare_path(cx: &mut gpui::TestAppContext) {
        crate::test_support::init_test(cx);

        let fs = FakeFs::new(cx.executor());
        fs.insert_tree(path!("/project"), json!({"src": {"main.rs": ""}}))
            .await;

        let project = Project::test(fs, [path!("/project").as_ref()], cx).await;
        let (multi_workspace, cx) =
            cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
        let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());
        let workspace_weak = workspace.downgrade();

        multi_workspace.update_in(cx, |_, window, cx| {
            open_link("src/main.rs".into(), &workspace_weak, window, cx);
        });
        cx.run_until_parked();
        workspace.read_with(cx, |workspace, cx| {
            let active = workspace
                .active_item(cx)
                .and_then(|item| item.project_path(cx))
                .expect("file should be open");
            assert!(*active.path == *"src/main.rs");
        });

        let abs_path: SharedString = path!("/project/src/main.rs").to_string().into();
        multi_workspace.update_in(cx, |_, window, cx| {
            open_link(abs_path, &workspace_weak, window, cx);
        });
        cx.run_until_parked();
        workspace.read_with(cx, |workspace, cx| {
            let active = workspace
                .active_item(cx)
                .and_then(|item| item.project_path(cx))
                .expect("file should be open");
            assert!(*active.path == *"src/main.rs");
        });
    }
}
