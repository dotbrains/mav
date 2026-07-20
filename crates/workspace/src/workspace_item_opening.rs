use super::*;

impl Workspace {
    pub fn add_item_to_center(
        &mut self,
        item: Box<dyn ItemHandle>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if let Some(center_pane) = self.last_tabbed_pane(cx) {
            center_pane.update(cx, |pane, cx| {
                pane.add_item(item, true, true, None, window, cx)
            });
            true
        } else {
            let center_pane = self.ensure_tabbed_pane(window, cx);
            center_pane.update(cx, |pane, cx| {
                pane.add_item(item, true, true, None, window, cx)
            });
            true
        }
    }

    pub fn add_item_to_active_pane(
        &mut self,
        item: Box<dyn ItemHandle>,
        destination_index: Option<usize>,
        focus_item: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let pane = if self.active_pane.read(cx).is_tabbed() {
            self.active_pane.clone()
        } else {
            self.last_tabbed_pane(cx)
                .unwrap_or_else(|| self.ensure_tabbed_pane(window, cx))
        };
        self.add_item(pane, item, destination_index, false, focus_item, window, cx)
    }

    pub fn add_item(
        &mut self,
        pane: Entity<Pane>,
        item: Box<dyn ItemHandle>,
        destination_index: Option<usize>,
        activate_pane: bool,
        focus_item: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let is_panel_item = item.downcast::<PanelItem>().is_some();
        let pane = if pane.read(cx).is_tabbed() || is_panel_item {
            pane
        } else {
            self.ensure_tabbed_pane(window, cx)
        };

        pane.update(cx, |pane, cx| {
            pane.add_item(
                item,
                activate_pane,
                focus_item,
                destination_index,
                window,
                cx,
            )
        });
    }

    pub fn split_item(
        &mut self,
        split_direction: SplitDirection,
        item: Box<dyn ItemHandle>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let new_pane = self.split_pane(self.active_pane.clone(), split_direction, window, cx);
        self.add_item(new_pane, item, None, true, true, window, cx);
    }

    pub fn open_abs_path(
        &mut self,
        abs_path: PathBuf,
        options: OpenOptions,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<Box<dyn ItemHandle>>> {
        cx.spawn_in(window, async move |workspace, cx| {
            let open_paths_task_result = workspace
                .update_in(cx, |workspace, window, cx| {
                    workspace.open_paths(vec![abs_path.clone()], options, None, window, cx)
                })
                .with_context(|| format!("open abs path {abs_path:?} task spawn"))?
                .await;
            anyhow::ensure!(
                open_paths_task_result.len() == 1,
                "open abs path {abs_path:?} task returned incorrect number of results"
            );
            match open_paths_task_result
                .into_iter()
                .next()
                .expect("ensured single task result")
            {
                Some(open_result) => {
                    open_result.with_context(|| format!("open abs path {abs_path:?} task join"))
                }
                None => anyhow::bail!("open abs path {abs_path:?} task returned None"),
            }
        })
    }

    pub fn split_abs_path(
        &mut self,
        abs_path: PathBuf,
        visible: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<Box<dyn ItemHandle>>> {
        let project_path_task =
            Workspace::project_path_for_path(self.project.clone(), &abs_path, visible, cx);
        cx.spawn_in(window, async move |this, cx| {
            let (_, path) = project_path_task.await?;
            this.update_in(cx, |this, window, cx| this.split_path(path, window, cx))?
                .await
        })
    }

    pub fn open_path(
        &mut self,
        path: impl Into<ProjectPath>,
        pane: Option<WeakEntity<Pane>>,
        focus_item: bool,
        window: &mut Window,
        cx: &mut App,
    ) -> Task<anyhow::Result<Box<dyn ItemHandle>>> {
        self.open_path_preview(path, pane, focus_item, false, true, window, cx)
    }

    pub fn open_path_in_tabbed_pane(
        &mut self,
        path: impl Into<ProjectPath>,
        pane: Option<WeakEntity<Pane>>,
        focus_item: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<Box<dyn ItemHandle>>> {
        self.open_path_preview_in_tabbed_pane(path, pane, focus_item, false, true, window, cx)
    }

    pub fn open_path_preview(
        &mut self,
        path: impl Into<ProjectPath>,
        pane: Option<WeakEntity<Pane>>,
        focus_item: bool,
        allow_preview: bool,
        activate: bool,
        window: &mut Window,
        cx: &mut App,
    ) -> Task<anyhow::Result<Box<dyn ItemHandle>>> {
        let Some(pane) = self.existing_tabbed_pane(pane, cx) else {
            return Task::ready(Err(anyhow!("no tabbed pane available")));
        };

        self.open_path_preview_in_pane(
            path.into(),
            pane.downgrade(),
            focus_item,
            allow_preview,
            activate,
            window,
            cx,
        )
    }

    pub fn open_path_preview_in_tabbed_pane(
        &mut self,
        path: impl Into<ProjectPath>,
        pane: Option<WeakEntity<Pane>>,
        focus_item: bool,
        allow_preview: bool,
        activate: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<Box<dyn ItemHandle>>> {
        let pane = pane
            .and_then(|pane| pane.upgrade())
            .filter(|pane| pane.read(cx).is_tabbed() && pane.read(cx).is_visible())
            .unwrap_or_else(|| self.ensure_tabbed_pane(window, cx));

        self.open_path_preview_in_pane(
            path.into(),
            pane.downgrade(),
            focus_item,
            allow_preview,
            activate,
            window,
            cx,
        )
    }

    pub(crate) fn open_path_preview_in_pane(
        &mut self,
        project_path: ProjectPath,
        pane: WeakEntity<Pane>,
        focus_item: bool,
        allow_preview: bool,
        activate: bool,
        window: &mut Window,
        cx: &mut App,
    ) -> Task<anyhow::Result<Box<dyn ItemHandle>>> {
        let task = self.load_path(project_path.clone(), window, cx);
        window.spawn(cx, async move |cx| {
            let (project_entry_id, build_item) = task.await?;

            pane.update_in(cx, |pane, window, cx| {
                pane.open_item(
                    project_entry_id,
                    project_path,
                    focus_item,
                    allow_preview,
                    activate,
                    None,
                    window,
                    cx,
                    build_item,
                )
            })
        })
    }
}
