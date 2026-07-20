use super::*;

impl Workspace {
    pub fn status_bar(&self) -> &Entity<StatusBar> {
        &self.status_bar
    }

    pub fn set_sidebar_focus_handle(&mut self, handle: Option<FocusHandle>) {
        self.sidebar_focus_handle = handle;
    }

    pub fn notify_panes(&self, cx: &mut App) {
        for pane in &self.panes {
            cx.notify(pane.entity_id());
        }
    }

    pub fn status_bar_visible(&self, cx: &App) -> bool {
        StatusBarSettings::get_global(cx).show
    }

    pub fn multi_workspace(&self) -> Option<&WeakEntity<MultiWorkspace>> {
        self.multi_workspace.as_ref()
    }

    pub fn set_multi_workspace(
        &mut self,
        multi_workspace: WeakEntity<MultiWorkspace>,
        active_workspace_id: Rc<Cell<EntityId>>,
        cx: &mut App,
    ) {
        self.status_bar.update(cx, |status_bar, cx| {
            status_bar.set_multi_workspace(multi_workspace.clone(), cx);
        });
        self.multi_workspace = Some(multi_workspace);
        self.active_workspace_id = Some(active_workspace_id);
    }

    pub fn app_state(&self) -> &Arc<AppState> {
        &self.app_state
    }

    pub fn set_panels_task(&mut self, task: Task<Result<()>>) {
        self._panels_task = Some(task);
    }

    pub fn take_panels_task(&mut self) -> Option<Task<Result<()>>> {
        self._panels_task.take()
    }

    pub fn user_store(&self) -> &Entity<UserStore> {
        &self.app_state.user_store
    }

    pub fn project(&self) -> &Entity<Project> {
        &self.project
    }

    pub fn path_style(&self, cx: &App) -> PathStyle {
        self.project.read(cx).path_style(cx)
    }

    pub fn recently_activated_items(&self, cx: &App) -> HashMap<EntityId, usize> {
        let mut history: HashMap<EntityId, usize> = HashMap::default();

        for pane_handle in &self.panes {
            let pane = pane_handle.read(cx);

            for entry in pane.activation_history() {
                history.insert(
                    entry.entity_id,
                    history
                        .get(&entry.entity_id)
                        .cloned()
                        .unwrap_or(0)
                        .max(entry.timestamp),
                );
            }
        }

        history
    }

    pub fn client(&self) -> &Arc<Client> {
        &self.app_state.client
    }

    pub fn set_prompt_for_new_path(&mut self, prompt: PromptForNewPath) {
        self.on_prompt_for_new_path = Some(prompt)
    }

    pub fn set_prompt_for_open_path(&mut self, prompt: PromptForOpenPath) {
        self.on_prompt_for_open_path = Some(prompt)
    }

    pub fn set_terminal_provider(&mut self, provider: impl TerminalProvider + 'static) {
        self.terminal_provider = Some(Box::new(provider));
    }

    pub fn set_debugger_provider(&mut self, provider: impl DebuggerProvider + 'static) {
        self.debugger_provider = Some(Arc::new(provider));
    }

    pub fn set_open_in_dev_container(&mut self, value: bool) {
        self.open_in_dev_container = value;
    }

    pub fn open_in_dev_container(&self) -> bool {
        self.open_in_dev_container
    }

    pub fn set_dev_container_task(&mut self, task: Task<Result<()>>) {
        self._dev_container_task = Some(task);
    }

    pub fn debugger_provider(&self) -> Option<Arc<dyn DebuggerProvider>> {
        self.debugger_provider.clone()
    }

    pub fn prompt_for_open_path(
        &mut self,
        path_prompt_options: PathPromptOptions,
        lister: DirectoryLister,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> oneshot::Receiver<Option<Vec<PathBuf>>> {
        // TODO: If `on_prompt_for_open_path` is set, we should always use it
        // rather than gating on `use_system_path_prompts`. This would let tests
        // inject a mock without also having to disable the setting.
        if !lister.is_local(cx) || !WorkspaceSettings::get_global(cx).use_system_path_prompts {
            let prompt = self.on_prompt_for_open_path.take().unwrap();
            let rx = prompt(self, lister, window, cx);
            self.on_prompt_for_open_path = Some(prompt);
            rx
        } else {
            let (tx, rx) = oneshot::channel();
            let abs_path = cx.prompt_for_paths(path_prompt_options);

            cx.spawn_in(window, async move |workspace, cx| {
                let Ok(result) = abs_path.await else {
                    return Ok(());
                };

                match result {
                    Ok(result) => {
                        tx.send(result).ok();
                    }
                    Err(err) => {
                        let rx = workspace.update_in(cx, |workspace, window, cx| {
                            workspace
                                .show_error(workspace_error::PortalError::new(err.to_string()), cx);
                            let prompt = workspace.on_prompt_for_open_path.take().unwrap();
                            let rx = prompt(workspace, lister, window, cx);
                            workspace.on_prompt_for_open_path = Some(prompt);
                            rx
                        })?;
                        if let Ok(path) = rx.await {
                            tx.send(path).ok();
                        }
                    }
                };
                anyhow::Ok(())
            })
            .detach();

            rx
        }
    }

    pub fn prompt_for_new_path(
        &mut self,
        lister: DirectoryLister,
        suggested_name: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> oneshot::Receiver<Option<Vec<PathBuf>>> {
        if self.project.read(cx).is_via_collab()
            || self.project.read(cx).is_via_remote_server()
            || !WorkspaceSettings::get_global(cx).use_system_path_prompts
        {
            let prompt = self.on_prompt_for_new_path.take().unwrap();
            let rx = prompt(self, lister, suggested_name, window, cx);
            self.on_prompt_for_new_path = Some(prompt);
            return rx;
        }

        let (tx, rx) = oneshot::channel();
        cx.spawn_in(window, async move |workspace, cx| {
            let abs_path = workspace.update(cx, |workspace, cx| {
                let relative_to = workspace
                    .most_recent_active_path(cx)
                    .and_then(|p| p.parent().map(|p| p.to_path_buf()))
                    .or_else(|| {
                        let project = workspace.project.read(cx);
                        project.visible_worktrees(cx).find_map(|worktree| {
                            Some(worktree.read(cx).as_local()?.abs_path().to_path_buf())
                        })
                    })
                    .or_else(std::env::home_dir)
                    .unwrap_or_else(|| PathBuf::from(""));
                cx.prompt_for_new_path(&relative_to, suggested_name.as_deref())
            })?;
            let abs_path = match abs_path.await? {
                Ok(path) => path,
                Err(err) => {
                    let rx = workspace.update_in(cx, |workspace, window, cx| {
                        workspace
                            .show_error(workspace_error::PortalError::new(err.to_string()), cx);

                        let prompt = workspace.on_prompt_for_new_path.take().unwrap();
                        let rx = prompt(workspace, lister, suggested_name, window, cx);
                        workspace.on_prompt_for_new_path = Some(prompt);
                        rx
                    })?;
                    if let Ok(path) = rx.await {
                        tx.send(path).ok();
                    }
                    return anyhow::Ok(());
                }
            };

            tx.send(abs_path.map(|path| vec![path])).ok();
            anyhow::Ok(())
        })
        .detach();

        rx
    }

    /// Call the given callback with a workspace whose project is local or remote via WSL (allowing host access).
    ///
    /// If the given workspace has a local project, then it will be passed
    /// to the callback. Otherwise, a new empty window will be created.
    pub fn with_local_workspace<T, F>(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        callback: F,
    ) -> Task<Result<T>>
    where
        T: 'static,
        F: 'static + FnOnce(&mut Workspace, &mut Window, &mut Context<Workspace>) -> T,
    {
        if self.project.read(cx).is_local() {
            Task::ready(Ok(callback(self, window, cx)))
        } else {
            let env = self.project.read(cx).cli_environment(cx);
            let task = Self::new_local(
                Vec::new(),
                self.app_state.clone(),
                None,
                env,
                None,
                OpenMode::Activate,
                cx,
            );
            cx.spawn_in(window, async move |_vh, cx| {
                let OpenResult {
                    window: multi_workspace_window,
                    ..
                } = task.await?;
                multi_workspace_window.update(cx, |multi_workspace, window, cx| {
                    let workspace = multi_workspace.workspace().clone();
                    workspace.update(cx, |workspace, cx| callback(workspace, window, cx))
                })
            })
        }
    }

    /// Call the given callback with a workspace whose project is local or remote via WSL (allowing host access).
    ///
    /// If the given workspace has a local project, then it will be passed
    /// to the callback. Otherwise, a new empty window will be created.
    pub fn with_local_or_wsl_workspace<T, F>(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        callback: F,
    ) -> Task<Result<T>>
    where
        T: 'static,
        F: 'static + FnOnce(&mut Workspace, &mut Window, &mut Context<Workspace>) -> T,
    {
        let project = self.project.read(cx);
        if project.is_local() || project.is_via_wsl_with_host_interop(cx) {
            Task::ready(Ok(callback(self, window, cx)))
        } else {
            let env = self.project.read(cx).cli_environment(cx);
            let task = Self::new_local(
                Vec::new(),
                self.app_state.clone(),
                None,
                env,
                None,
                OpenMode::Activate,
                cx,
            );
            cx.spawn_in(window, async move |_vh, cx| {
                let OpenResult {
                    window: multi_workspace_window,
                    ..
                } = task.await?;
                multi_workspace_window.update(cx, |multi_workspace, window, cx| {
                    let workspace = multi_workspace.workspace().clone();
                    workspace.update(cx, |workspace, cx| callback(workspace, window, cx))
                })
            })
        }
    }

    pub fn worktrees<'a>(&self, cx: &'a App) -> impl 'a + Iterator<Item = Entity<Worktree>> {
        self.project.read(cx).worktrees(cx)
    }

    pub fn visible_worktrees<'a>(
        &self,
        cx: &'a App,
    ) -> impl 'a + Iterator<Item = Entity<Worktree>> {
        self.project.read(cx).visible_worktrees(cx)
    }

    pub fn worktree_scans_complete(&self, cx: &App) -> impl Future<Output = ()> + 'static + use<> {
        let futures = self
            .worktrees(cx)
            .filter_map(|worktree| worktree.read(cx).as_local())
            .map(|worktree| worktree.scan_complete())
            .collect::<Vec<_>>();
        async move {
            for future in futures {
                future.await;
            }
        }
    }
}
