use super::*;

impl GitPanel {
    pub fn load_commit_details(
        &self,
        sha: String,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<CommitDetails>> {
        let Some(repo) = self.active_repository.clone() else {
            return Task::ready(Err(anyhow::anyhow!("no active repo")));
        };
        repo.update(cx, |repo, cx| {
            let show = repo.show(sha);
            cx.spawn(async move |_, _| show.await?)
        })
    }

    pub(super) fn deploy_entry_context_menu(
        &mut self,
        position: Point<Pixels>,
        ix: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(entry) = self.entries.get(ix).and_then(|e| e.status_entry()) else {
            return;
        };
        let stage_title = if entry.status.staging().is_fully_staged() {
            "Unstage File"
        } else {
            "Stage File"
        };
        let restore_title = if entry.status.is_created() {
            "Trash File"
        } else {
            "Discard Changes"
        };
        let context_menu = ContextMenu::build(window, cx, |context_menu, _, _| {
            let is_created = entry.status.is_created();
            context_menu
                .context(self.focus_handle.clone())
                .action(stage_title, ToggleStaged.boxed_clone())
                .action(restore_title, git::RestoreFile::default().boxed_clone())
                .separator()
                .action_disabled_when(
                    !is_created,
                    "Add to .gitignore",
                    git::AddToGitignore.boxed_clone(),
                )
                .action_disabled_when(
                    !is_created,
                    "Add to .git/info/exclude",
                    git::AddToGitInfoExclude.boxed_clone(),
                )
                .separator()
                .action("Open Diff", menu::Confirm.boxed_clone())
                .action("Open Diff (File)", menu::SecondaryConfirm.boxed_clone())
                .action("View File", ViewFile.boxed_clone())
                .when(!is_created, |context_menu| {
                    context_menu
                        .separator()
                        .action("View File History", Box::new(git::FileHistory))
                })
        });
        self.selected_entry = Some(ix);
        self.set_context_menu(context_menu, position, window, cx);
    }

    pub(super) fn deploy_panel_context_menu(
        &mut self,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let has_tracked_changes = self.has_tracked_changes();
        let has_staged_changes = self.has_staged_changes();
        let has_unstaged_changes = self.has_unstaged_changes();
        let has_new_changes = self.new_count > 0;
        let has_stash_items = self.stash_entries.entries.len() > 0;

        let context_menu = git_panel_context_menu(
            has_tracked_changes,
            has_staged_changes,
            has_unstaged_changes,
            has_new_changes,
            has_stash_items,
            self.focus_handle.clone(),
            window,
            cx,
        );
        self.set_context_menu(context_menu, position, window, cx);
    }

    pub(super) fn set_context_menu(
        &mut self,
        context_menu: Entity<ContextMenu>,
        position: Point<Pixels>,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let subscription = cx.subscribe_in(
            &context_menu,
            window,
            |this, _, _: &DismissEvent, window, cx| {
                if this.context_menu.as_ref().is_some_and(|context_menu| {
                    context_menu.0.focus_handle(cx).contains_focused(window, cx)
                }) {
                    cx.focus_self(window);
                }
                this.context_menu.take();
                cx.notify();
            },
        );
        self.context_menu = Some((context_menu, position, subscription));
        cx.notify();
    }

    pub(super) fn has_write_access(&self, cx: &App) -> bool {
        !self.project.read(cx).is_read_only(cx)
    }
}
