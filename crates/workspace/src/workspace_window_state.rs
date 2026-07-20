use super::*;

impl Workspace {
    /// Whether this workspace may write the platform window's title and edited
    /// indicator.
    ///
    /// In a multi-workspace window several workspaces share one platform
    /// window, so only the active one is allowed to write that chrome -
    /// otherwise a background workspace's project/item events would clobber the
    /// active workspace's title. `MultiWorkspace` publishes the active
    /// workspace's id into the shared `active_workspace_id` cell, which we
    /// simply compare against our own id. A workspace with no shared cell (e.g.
    /// a plain test window) owns its window unconditionally.
    pub(crate) fn owns_window_chrome(&self) -> bool {
        match &self.active_workspace_id {
            Some(active_workspace_id) => active_workspace_id.get() == self.weak_self.entity_id(),
            None => true,
        }
    }

    pub(crate) fn update_window_title(&mut self, window: &mut Window, cx: &mut App) {
        if !self.owns_window_chrome() {
            return;
        }
        self.apply_window_title(window, cx);
    }

    pub(crate) fn apply_window_title(&mut self, window: &mut Window, cx: &mut App) {
        let project = self.project().read(cx);
        let mut title = String::new();

        for (i, worktree) in project.visible_worktrees(cx).enumerate() {
            let name = worktree.read(cx).root_name_str();

            if i > 0 {
                title.push_str(", ");
            }
            title.push_str(name);
        }

        if title.is_empty() {
            title = "empty project".to_string();
        }

        let active_project_path = self.active_item(cx).and_then(|item| item.project_path(cx));

        if let Some(path) = active_project_path.as_ref() {
            let filename = path.path.file_name().or_else(|| {
                Some(
                    project
                        .worktree_for_id(path.worktree_id, cx)?
                        .read(cx)
                        .root_name_str(),
                )
            });

            if let Some(filename) = filename {
                title.push_str(" — ");
                title.push_str(filename.as_ref());
            }
        }

        if project.is_via_collab() {
            title.push_str(" ↙");
        } else if project.is_shared() {
            title.push_str(" ↗");
        }

        let document_path = active_project_path
            .as_ref()
            .and_then(|path| project.absolute_path(path, cx));
        window.set_document_path(document_path.as_deref());

        if let Some(last_title) = self.last_window_title.as_ref()
            && &title == last_title
        {
            return;
        }
        window.set_window_title(&title);
        SystemWindowTabController::update_tab_title(
            cx,
            window.window_handle().window_id(),
            SharedString::from(&title),
        );
        self.last_window_title = Some(title);
    }

    pub(crate) fn is_window_edited(&self, cx: &App) -> bool {
        !self.project.read(cx).is_disconnected(cx) && !self.dirty_items.is_empty()
    }

    pub(crate) fn update_window_edited(&mut self, window: &mut Window, cx: &mut App) {
        if !self.owns_window_chrome() {
            return;
        }
        let is_edited = self.is_window_edited(cx);
        if is_edited != self.window_edited {
            self.window_edited = is_edited;
            window.set_window_edited(self.window_edited)
        }
    }

    /// Re-applies this workspace's title and edited indicator to the platform
    /// window, bypassing the change-detection caches.
    ///
    /// Several workspaces can share a single platform window (a multi-workspace
    /// window). The `last_window_title`/`window_edited` caches assume the
    /// window's title and edited state reflect *this* workspace, but after the
    /// active workspace changes those values may have been set by a different
    /// workspace. Clearing the caches forces the values to be re-applied so the
    /// window reflects the newly-active workspace.
    ///
    /// This is only ever invoked for the workspace that just became active, so
    /// it writes directly rather than going through `update_window_title` /
    /// `update_window_edited`, whose change-detection caches would otherwise
    /// suppress the write when this workspace's computed title/edited state
    /// matches what *it* last set (even though the shared window currently
    /// shows a different workspace's state).
    pub fn refresh_window_state(&mut self, window: &mut Window, cx: &mut App) {
        self.last_window_title = None;
        self.apply_window_title(window, cx);

        self.window_edited = self.is_window_edited(cx);
        window.set_window_edited(self.window_edited);
    }

    pub(crate) fn update_item_dirty_state(
        &mut self,
        item: &dyn ItemHandle,
        window: &mut Window,
        cx: &mut App,
    ) {
        let is_dirty = item.is_dirty(cx);
        let item_id = item.item_id();
        let was_dirty = self.dirty_items.contains_key(&item_id);
        if is_dirty == was_dirty {
            return;
        }
        if was_dirty {
            self.dirty_items.remove(&item_id);
            self.update_window_edited(window, cx);
            return;
        }

        let workspace = self.weak_handle();
        let Some(window_handle) = window.window_handle().downcast::<MultiWorkspace>() else {
            return;
        };
        let on_release_callback = Box::new(move |cx: &mut App| {
            window_handle
                .update(cx, |_, window, cx| {
                    workspace
                        .update(cx, |workspace, cx| {
                            workspace.dirty_items.remove(&item_id);
                            workspace.update_window_edited(window, cx)
                        })
                        .ok();
                })
                .ok();
        });

        let s = item.on_release(cx, on_release_callback);
        self.dirty_items.insert(item_id, s);
        self.update_window_edited(window, cx);
    }
}
