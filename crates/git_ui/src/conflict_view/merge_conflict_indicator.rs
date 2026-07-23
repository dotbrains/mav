use super::*;

fn collect_conflicted_file_paths(project: &Project, cx: &App) -> Vec<String> {
    let git_store = project.git_store().read(cx);
    let mut paths = Vec::new();

    for repo in git_store.repositories().values() {
        let snapshot = repo.read(cx).snapshot();
        for (repo_path, _) in snapshot.merge.merge_heads_by_conflicted_path.iter() {
            let is_currently_conflicted = snapshot
                .status_for_path(repo_path)
                .is_some_and(|entry| entry.status.is_conflicted());
            if !is_currently_conflicted {
                continue;
            }
            if let Some(project_path) = repo.read(cx).repo_path_to_project_path(repo_path, cx) {
                paths.push(
                    project_path
                        .path
                        .as_std_path()
                        .to_string_lossy()
                        .to_string(),
                );
            }
        }
    }

    paths
}

pub struct MergeConflictIndicator {
    project: Entity<Project>,
    conflicted_paths: Vec<String>,
    last_shown_paths: HashSet<String>,
    dismissed: bool,
    _subscription: Subscription,
}

impl MergeConflictIndicator {
    pub fn new(workspace: &Workspace, cx: &mut Context<Self>) -> Self {
        let project = workspace.project().clone();
        let git_store = project.read(cx).git_store().clone();

        let subscription = cx.subscribe(&git_store, Self::on_git_store_event);

        let conflicted_paths = collect_conflicted_file_paths(project.read(cx), cx);
        let last_shown_paths: HashSet<String> = conflicted_paths.iter().cloned().collect();

        Self {
            project,
            conflicted_paths,
            last_shown_paths,
            dismissed: false,
            _subscription: subscription,
        }
    }

    fn on_git_store_event(
        &mut self,
        _git_store: Entity<GitStore>,
        event: &GitStoreEvent,
        cx: &mut Context<Self>,
    ) {
        let conflicts_changed = matches!(
            event,
            GitStoreEvent::ConflictsUpdated
                | GitStoreEvent::RepositoryUpdated(_, RepositoryEvent::StatusesChanged, _)
        );

        let agent_settings = AgentSettings::get_global(cx);
        if !agent_settings.enabled(cx)
            || !agent_settings.show_merge_conflict_indicator
            || !conflicts_changed
        {
            return;
        }

        let project = self.project.read(cx);
        if project.is_via_collab() {
            return;
        }

        let paths = collect_conflicted_file_paths(project, cx);
        let current_paths_set: HashSet<String> = paths.iter().cloned().collect();

        if paths.is_empty() {
            self.conflicted_paths.clear();
            self.last_shown_paths.clear();
            self.dismissed = false;
            cx.notify();
        } else if self.last_shown_paths != current_paths_set {
            self.last_shown_paths = current_paths_set;
            self.conflicted_paths = paths;
            self.dismissed = false;
            cx.notify();
        }
    }

    fn resolve_with_agent(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.dispatch_action(
            Box::new(ResolveConflictedFilesWithAgent {
                conflicted_file_paths: self.conflicted_paths.clone(),
            }),
            cx,
        );
        self.dismissed = true;
        cx.notify();
    }

    fn dismiss(&mut self, _: &ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        self.dismissed = true;
        cx.notify();
    }
}

impl Render for MergeConflictIndicator {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let agent_settings = AgentSettings::get_global(cx);
        if !agent_settings.enabled(cx)
            || !agent_settings.show_merge_conflict_indicator
            || self.conflicted_paths.is_empty()
            || self.dismissed
        {
            return Empty.into_any_element();
        }

        let file_count = self.conflicted_paths.len();

        let message: SharedString = format!(
            "Resolve Merge Conflict{} with Agent",
            if file_count == 1 { "" } else { "s" }
        )
        .into();

        let tooltip_label: SharedString = format!(
            "Found {} {} across the codebase",
            file_count,
            if file_count == 1 {
                "conflict"
            } else {
                "conflicts"
            }
        )
        .into();

        let border_color = cx.theme().colors().text_accent.opacity(0.2);

        h_flex()
            .h(rems_from_px(22.))
            .rounded_sm()
            .border_1()
            .border_color(border_color)
            .child(
                ButtonLike::new("update-button")
                    .child(
                        h_flex()
                            .h_full()
                            .gap_1()
                            .child(
                                Icon::new(IconName::GitMergeConflict)
                                    .size(IconSize::Small)
                                    .color(Color::Muted),
                            )
                            .child(Label::new(message).size(LabelSize::Small)),
                    )
                    .tooltip(move |_, cx| {
                        Tooltip::with_meta(
                            tooltip_label.clone(),
                            None,
                            "Click to Resolve with Agent",
                            cx,
                        )
                    })
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.resolve_with_agent(window, cx);
                    })),
            )
            .child(
                div().border_l_1().border_color(border_color).child(
                    IconButton::new("dismiss-merge-conflicts", IconName::Close)
                        .icon_size(IconSize::XSmall)
                        .on_click(cx.listener(Self::dismiss)),
                ),
            )
            .into_any_element()
    }
}

impl StatusItemView for MergeConflictIndicator {
    fn set_active_pane_item(
        &mut self,
        _: Option<&dyn ItemHandle>,
        _window: &mut Window,
        _: &mut Context<Self>,
    ) {
    }

    fn hide_setting(&self, _: &App) -> Option<HideStatusItem> {
        Some(HideStatusItem::new(|settings| {
            settings
                .agent
                .get_or_insert_default()
                .show_merge_conflict_indicator = Some(false);
        }))
    }
}
