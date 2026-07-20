use super::*;

impl Workspace {
    pub fn recent_active_item_by_type<T: 'static>(&self, cx: &App) -> Option<Entity<T>> {
        let mut recent_item: Option<Entity<T>> = None;
        let mut recent_timestamp = 0;
        for pane_handle in &self.panes {
            let pane = pane_handle.read(cx);
            let item_map: HashMap<EntityId, &Box<dyn ItemHandle>> =
                pane.items().map(|item| (item.item_id(), item)).collect();
            for entry in pane.activation_history() {
                if entry.timestamp > recent_timestamp
                    && let Some(&item) = item_map.get(&entry.entity_id)
                    && let Some(typed_item) = item.act_as::<T>(cx)
                {
                    recent_timestamp = entry.timestamp;
                    recent_item = Some(typed_item);
                }
            }
        }
        recent_item
    }

    pub fn recent_navigation_history_iter(
        &self,
        cx: &App,
    ) -> impl Iterator<Item = (ProjectPath, Option<PathBuf>)> + use<> {
        let mut abs_paths_opened: HashMap<PathBuf, HashSet<ProjectPath>> = HashMap::default();
        let mut history: HashMap<ProjectPath, (Option<PathBuf>, usize)> = HashMap::default();

        for pane in &self.panes {
            let pane = pane.read(cx);

            pane.nav_history()
                .for_each_entry(cx, &mut |entry, (project_path, fs_path)| {
                    if let Some(fs_path) = &fs_path {
                        abs_paths_opened
                            .entry(fs_path.clone())
                            .or_default()
                            .insert(project_path.clone());
                    }
                    let timestamp = entry.timestamp;
                    match history.entry(project_path) {
                        hash_map::Entry::Occupied(mut entry) => {
                            let (_, old_timestamp) = entry.get();
                            if &timestamp > old_timestamp {
                                entry.insert((fs_path, timestamp));
                            }
                        }
                        hash_map::Entry::Vacant(entry) => {
                            entry.insert((fs_path, timestamp));
                        }
                    }
                });

            if let Some(item) = pane.active_item()
                && let Some(project_path) = item.project_path(cx)
            {
                let fs_path = self.project.read(cx).absolute_path(&project_path, cx);

                if let Some(fs_path) = &fs_path {
                    abs_paths_opened
                        .entry(fs_path.clone())
                        .or_default()
                        .insert(project_path.clone());
                }

                history.insert(project_path, (fs_path, std::usize::MAX));
            }
        }

        history
            .into_iter()
            .sorted_by_key(|(_, (_, order))| *order)
            .map(|(project_path, (fs_path, _))| (project_path, fs_path))
            .rev()
            .filter(move |(history_path, abs_path)| {
                let latest_project_path_opened = abs_path
                    .as_ref()
                    .and_then(|abs_path| abs_paths_opened.get(abs_path))
                    .and_then(|project_paths| {
                        project_paths
                            .iter()
                            .max_by(|b1, b2| b1.worktree_id.cmp(&b2.worktree_id))
                    });

                latest_project_path_opened.is_none_or(|path| path == history_path)
            })
    }

    pub fn recent_navigation_history(
        &self,
        limit: Option<usize>,
        cx: &App,
    ) -> Vec<(ProjectPath, Option<PathBuf>)> {
        self.recent_navigation_history_iter(cx)
            .take(limit.unwrap_or(usize::MAX))
            .collect()
    }

    pub fn clear_navigation_history(&mut self, _window: &mut Window, cx: &mut Context<Workspace>) {
        for pane in &self.panes {
            pane.update(cx, |pane, cx| pane.nav_history_mut().clear(cx));
        }
    }

    fn navigate_history(
        &mut self,
        pane: WeakEntity<Pane>,
        mode: NavigationMode,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) -> Task<Result<()>> {
        self.navigate_history_impl(
            pane,
            mode,
            window,
            &mut |history, cx| history.pop(mode, cx),
            cx,
        )
    }

    pub(crate) fn navigate_tag_history(
        &mut self,
        pane: WeakEntity<Pane>,
        mode: TagNavigationMode,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) -> Task<Result<()>> {
        self.navigate_history_impl(
            pane,
            NavigationMode::Normal,
            window,
            &mut |history, _cx| history.pop_tag(mode),
            cx,
        )
    }

    fn navigate_history_impl(
        &mut self,
        pane: WeakEntity<Pane>,
        mode: NavigationMode,
        window: &mut Window,
        cb: &mut dyn FnMut(&mut NavHistory, &mut App) -> Option<NavigationEntry>,
        cx: &mut Context<Workspace>,
    ) -> Task<Result<()>> {
        let to_load = if let Some(pane) = pane.upgrade() {
            pane.update(cx, |pane, cx| {
                window.focus(&pane.focus_handle(cx), cx);
                loop {
                    // Retrieve the weak item handle from the history.
                    let entry = cb(pane.nav_history_mut(), cx)?;

                    // If the item is still present in this pane, then activate it.
                    if let Some(index) = entry
                        .item
                        .upgrade()
                        .and_then(|v| pane.index_for_item(v.as_ref()))
                    {
                        let prev_active_item_index = pane.active_item_index();
                        pane.nav_history_mut().set_mode(mode);
                        pane.activate_item(index, true, true, window, cx);
                        pane.nav_history_mut().set_mode(NavigationMode::Normal);

                        let mut navigated = prev_active_item_index != pane.active_item_index();
                        if let Some(data) = entry.data {
                            navigated |= pane.active_item()?.navigate(data, window, cx);
                        }

                        if navigated {
                            break None;
                        }
                    } else {
                        // If the item is no longer present in this pane, then retrieve its
                        // path info in order to reopen it.
                        break pane
                            .nav_history()
                            .path_for_item(entry.item.id())
                            .map(|(project_path, abs_path)| (project_path, abs_path, entry));
                    }
                }
            })
        } else {
            None
        };

        if let Some((project_path, abs_path, entry)) = to_load {
            // If the item was no longer present, then load it again from its previous path, first try the local path
            let open_by_project_path = self.load_path(project_path.clone(), window, cx);

            cx.spawn_in(window, async move  |workspace, cx| {
                let open_by_project_path = open_by_project_path.await;
                let mut navigated = false;
                match open_by_project_path
                    .with_context(|| format!("Navigating to {project_path:?}"))
                {
                    Ok((project_entry_id, build_item)) => {
                        let prev_active_item_id = pane.update(cx, |pane, _| {
                            pane.nav_history_mut().set_mode(mode);
                            pane.active_item().map(|p| p.item_id())
                        })?;

                        pane.update_in(cx, |pane, window, cx| {
                            let item = pane.open_item(
                                project_entry_id,
                                project_path,
                                true,
                                entry.is_preview,
                                true,
                                None,
                                window, cx,
                                build_item,
                            );
                            navigated |= Some(item.item_id()) != prev_active_item_id;
                            pane.nav_history_mut().set_mode(NavigationMode::Normal);
                            if let Some(data) = entry.data {
                                navigated |= item.navigate(data, window, cx);
                            }
                        })?;
                    }
                    Err(open_by_project_path_e) => {
                        // Fall back to opening by abs path, in case an external file was opened and closed,
                        // and its worktree is now dropped
                        if let Some(abs_path) = abs_path {
                            let prev_active_item_id = pane.update(cx, |pane, _| {
                                pane.nav_history_mut().set_mode(mode);
                                pane.active_item().map(|p| p.item_id())
                            })?;
                            let open_by_abs_path = workspace.update_in(cx, |workspace, window, cx| {
                                workspace.open_abs_path(abs_path.clone(), OpenOptions { visible: Some(OpenVisible::None), ..Default::default() }, window, cx)
                            })?;
                            match open_by_abs_path
                                .await
                                .with_context(|| format!("Navigating to {abs_path:?}"))
                            {
                                Ok(item) => {
                                    pane.update_in(cx, |pane, window, cx| {
                                        navigated |= Some(item.item_id()) != prev_active_item_id;
                                        pane.nav_history_mut().set_mode(NavigationMode::Normal);
                                        if let Some(data) = entry.data {
                                            navigated |= item.navigate(data, window, cx);
                                        }
                                    })?;
                                }
                                Err(open_by_abs_path_e) => {
                                    log::error!("Failed to navigate history: {open_by_project_path_e:#} and {open_by_abs_path_e:#}");
                                }
                            }
                        }
                    }
                }

                if !navigated {
                    workspace
                        .update_in(cx, |workspace, window, cx| {
                            Self::navigate_history(workspace, pane, mode, window, cx)
                        })?
                        .await?;
                }

                Ok(())
            })
        } else {
            Task::ready(Ok(()))
        }
    }

    pub fn go_back(
        &mut self,
        pane: WeakEntity<Pane>,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) -> Task<Result<()>> {
        self.navigate_history(pane, NavigationMode::GoingBack, window, cx)
    }

    pub fn go_forward(
        &mut self,
        pane: WeakEntity<Pane>,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) -> Task<Result<()>> {
        self.navigate_history(pane, NavigationMode::GoingForward, window, cx)
    }

    pub fn reopen_closed_item(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) -> Task<Result<()>> {
        self.navigate_history(
            self.active_pane().downgrade(),
            NavigationMode::ReopeningClosedItem,
            window,
            cx,
        )
    }
}
