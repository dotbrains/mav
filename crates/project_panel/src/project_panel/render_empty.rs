use super::*;

impl ProjectPanel {
    pub(super) fn render_empty_project_panel(
        &mut self,
        is_local: bool,
        drag_and_drop_enabled: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let focus_handle = self.focus_handle(cx);
        let workspace = self.workspace.clone();
        let workspace_clone = self.workspace.clone();

        v_flex()
            .id("empty-project_panel-wrapper")
            .size_full()
            .bg(cx.theme().colors().editor_background)
            .child(
                ProjectEmptyState::new(
                    "Project Panel",
                    focus_handle.clone(),
                    KeyBinding::for_action_in(&workspace::Open::default(), &focus_handle, cx),
                )
                .on_open_project(move |_, window, cx| {
                    telemetry::event!("Project Panel Add Project Clicked");
                    workspace
                        .update(cx, |_, cx| {
                            window.dispatch_action(workspace::Open::default().boxed_clone(), cx);
                        })
                        .log_err();
                })
                .on_clone_repo(move |_, window, cx| {
                    telemetry::event!("Project Panel Clone Repo Clicked");
                    workspace_clone
                        .update(cx, |_, cx| {
                            window.dispatch_action(git::Clone.boxed_clone(), cx);
                        })
                        .log_err();
                }),
            )
            .when(is_local, |div| {
                div.when(drag_and_drop_enabled, |div| {
                    div.drag_over::<ExternalPaths>(|style, _, _, cx| {
                        style.bg(cx.theme().colors().drop_target_background)
                    })
                    .on_drop(cx.listener(
                        move |this, external_paths: &ExternalPaths, window, cx| {
                            this.drag_target_entry = None;
                            this.hover_scroll_task.take();
                            if let Some(task) = this
                                .workspace
                                .update(cx, |workspace, cx| {
                                    workspace.open_workspace_for_paths(
                                        OpenMode::Activate,
                                        external_paths.paths().to_owned(),
                                        window,
                                        cx,
                                    )
                                })
                                .log_err()
                            {
                                task.detach_and_log_err(cx);
                            }
                            cx.stop_propagation();
                        },
                    ))
                })
            })
    }
}
