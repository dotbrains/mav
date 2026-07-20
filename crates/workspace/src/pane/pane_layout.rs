use super::*;

impl Pane {
    pub fn focus_active_item(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(active_item) = self.active_item() {
            let focus_handle = active_item.item_focus_handle(cx);
            window.focus(&focus_handle, cx);
        }
    }

    pub fn split(
        &mut self,
        direction: SplitDirection,
        mode: SplitMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.items.len() <= 1 && mode == SplitMode::MovePane {
            // MovePane with only one pane present behaves like a SplitEmpty in the opposite direction
            let active_item = self.active_item();
            cx.emit(Event::Split {
                direction: direction.opposite(),
                mode: SplitMode::EmptyPane,
            });
            // ensure that we focus the moved pane
            // in this case we know that the window is the same as the active_item
            if let Some(active_item) = active_item {
                cx.defer_in(window, move |_, window, cx| {
                    let focus_handle = active_item.item_focus_handle(cx);
                    window.focus(&focus_handle, cx);
                });
            }
        } else {
            cx.emit(Event::Split { direction, mode });
        }
    }

    pub fn toolbar(&self) -> &Entity<Toolbar> {
        &self.toolbar
    }

    pub fn handle_deleted_project_item(
        &mut self,
        entry_id: ProjectEntryId,
        window: &mut Window,
        cx: &mut Context<Pane>,
    ) -> Option<()> {
        let item_id = self.items().find_map(|item| {
            if item.buffer_kind(cx) == ItemBufferKind::Singleton
                && item.project_entry_ids(cx).as_slice() == [entry_id]
            {
                Some(item.item_id())
            } else {
                None
            }
        })?;

        self.remove_item(item_id, false, true, window, cx);
        self.nav_history.remove_item(item_id);

        Some(())
    }

    pub(super) fn update_toolbar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let active_item = self
            .items
            .get(self.active_item_index)
            .map(|item| item.as_ref());
        self.toolbar.update(cx, |toolbar, cx| {
            toolbar.set_active_item(active_item, window, cx);
        });
    }

    pub(super) fn update_status_bar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let workspace = self.workspace.clone();
        let pane = cx.entity();

        window.defer(cx, move |window, cx| {
            let Ok(status_bar) =
                workspace.read_with(cx, |workspace, _| workspace.status_bar.clone())
            else {
                return;
            };

            status_bar.update(cx, move |status_bar, cx| {
                status_bar.set_active_pane(&pane, window, cx);
            });
        });
    }

    pub(super) fn entry_abs_path(&self, entry: ProjectEntryId, cx: &App) -> Option<PathBuf> {
        let worktree = self
            .workspace
            .upgrade()?
            .read(cx)
            .project()
            .read(cx)
            .worktree_for_entry(entry, cx)?
            .read(cx);
        let entry = worktree.entry_for_id(entry)?;
        Some(match &entry.canonical_path {
            Some(canonical_path) => canonical_path.to_path_buf(),
            None => worktree.absolutize(&entry.path),
        })
    }

    pub fn icon_color(selected: bool) -> Color {
        if selected {
            Color::Default
        } else {
            Color::Muted
        }
    }

    pub fn set_zoomed(&mut self, zoomed: bool, cx: &mut Context<Self>) {
        self.zoomed = zoomed;
        cx.notify();
    }

    pub fn is_zoomed(&self) -> bool {
        self.zoomed
    }
}
