use super::*;

#[derive(Debug)]
struct WorktreeFetchError {
    remote_name: String,
    branch_name: String,
    source: anyhow::Error,
}

impl WorktreeFetchError {
    fn remote_branch_name(&self) -> String {
        format!("{}/{}", self.remote_name, self.branch_name)
    }

    fn output(&self) -> String {
        format!("git fetch {} failed:\n{:#}", self.remote_name, self.source)
    }
}

impl fmt::Display for WorktreeFetchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "git fetch {} failed while creating worktree from {}: {}",
            self.remote_name,
            self.remote_branch_name(),
            self.source
        )
    }
}

impl Error for WorktreeFetchError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.source.as_ref())
    }
}

struct WorktreeFetchFailedToast {
    workspace: WeakEntity<Workspace>,
    worktree_name: Option<String>,
    branch_target: NewWorktreeBranchTarget,
    focused_dock: Option<DockPosition>,
    remote_branch_name: String,
    operation: SharedString,
    output: String,
    focus_handle: FocusHandle,
}

impl WorktreeFetchFailedToast {
    fn new(
        workspace: WeakEntity<Workspace>,
        worktree_name: Option<String>,
        branch_target: NewWorktreeBranchTarget,
        focused_dock: Option<DockPosition>,
        fetch_error: &WorktreeFetchError,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        Self {
            workspace,
            worktree_name,
            branch_target,
            focused_dock,
            remote_branch_name: fetch_error.remote_branch_name(),
            operation: format!("fetch {}", fetch_error.remote_name).into(),
            output: fetch_error.output(),
            focus_handle: cx.focus_handle(),
        }
    }
}

impl Focusable for WorktreeFetchFailedToast {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<DismissEvent> for WorktreeFetchFailedToast {}

impl ToastView for WorktreeFetchFailedToast {
    fn action(&self) -> Option<workspace::ToastAction> {
        None
    }

    fn auto_dismiss(&self) -> bool {
        false
    }
}

impl Render for WorktreeFetchFailedToast {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let workspace_for_retry = self.workspace.clone();
        let worktree_name = self.worktree_name.clone();
        let branch_target = self.branch_target.clone();
        let focused_dock = self.focused_dock;

        let workspace_for_log = self.workspace.clone();
        let operation = self.operation.clone();
        let output = self.output.clone();

        h_flex()
            .id("worktree-fetch-failed-toast")
            .elevation_3(cx)
            .gap_2()
            .py_1p5()
            .pl_2p5()
            .pr_1p5()
            .flex_none()
            .bg(cx.theme().colors().surface_background)
            .shadow_lg()
            .child(
                Icon::new(IconName::XCircle)
                    .size(IconSize::Small)
                    .color(Color::Error),
            )
            .child(Label::new(format!(
                "git fetch failed for {}",
                self.remote_branch_name
            )))
            .child(
                Button::new(
                    "use-local-worktree-base",
                    format!("Use local {}", self.remote_branch_name),
                )
                .color(Color::Muted)
                .on_click(cx.listener(move |_, _event, window, cx| {
                    cx.emit(DismissEvent);
                    if let Some(workspace) = workspace_for_retry.upgrade() {
                        workspace.update(cx, |workspace, cx| {
                            let task = create_worktree_workspace_inner(
                                workspace,
                                &mav_actions::CreateWorktree {
                                    worktree_name: worktree_name.clone(),
                                    branch_target: branch_target.clone(),
                                },
                                window,
                                focused_dock,
                                RemoteBranchFetchMode::UseLocal,
                                // User-initiated retry of a foreground create.
                                true,
                                cx,
                            );
                            task.detach_and_log_err(cx);
                        });
                    }
                })),
            )
            .child(
                Button::new("view-worktree-fetch-log", "Show Error Logs")
                    .color(Color::Muted)
                    .on_click(cx.listener(move |_, _event, window, cx| {
                        cx.emit(DismissEvent);
                        let output = output.clone();
                        let operation = operation.clone();
                        workspace_for_log
                            .update(cx, move |workspace, cx| {
                                open_output(operation, workspace, &output, window, cx)
                            })
                            .ok();
                    })),
            )
            .child(
                IconButton::new("dismiss-worktree-fetch-failed-toast", IconName::Close)
                    .shape(ui::IconButtonShape::Square)
                    .icon_size(IconSize::Small)
                    .icon_color(Color::Muted)
                    .on_click(cx.listener(|_, _event, _window, cx| {
                        cx.emit(DismissEvent);
                    })),
            )
    }
}
