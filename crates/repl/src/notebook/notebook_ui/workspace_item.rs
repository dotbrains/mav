use super::*;

impl Item for NotebookEditor {
    type Event = ();

    fn can_split(&self) -> bool {
        true
    }

    fn clone_on_split(
        &self,
        _workspace_id: Option<workspace::WorkspaceId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Option<Entity<Self>>>
    where
        Self: Sized,
    {
        Task::ready(Some(cx.new(|cx| {
            Self::new(self.project.clone(), self.notebook_item.clone(), window, cx)
        })))
    }

    fn buffer_kind(&self, _: &App) -> workspace::item::ItemBufferKind {
        workspace::item::ItemBufferKind::Singleton
    }

    fn for_each_project_item(
        &self,
        cx: &App,
        f: &mut dyn FnMut(gpui::EntityId, &dyn project::ProjectItem),
    ) {
        f(self.notebook_item.entity_id(), self.notebook_item.read(cx))
    }

    fn tab_content_text(&self, _detail: usize, cx: &App) -> SharedString {
        self.notebook_item
            .read(cx)
            .project_path
            .path
            .file_name()
            .map(|s| s.to_string())
            .unwrap_or_default()
            .into()
    }

    fn tab_content(&self, params: TabContentParams, window: &Window, cx: &App) -> AnyElement {
        Label::new(self.tab_content_text(params.detail.unwrap_or(0), cx))
            .single_line()
            .color(params.text_color())
            .when(params.preview, |this| this.italic())
            .into_any_element()
    }

    fn tab_icon(&self, _window: &Window, _cx: &App) -> Option<Icon> {
        Some(IconName::Book.into())
    }

    fn show_toolbar(&self) -> bool {
        false
    }

    // TODO
    fn pixel_position_of_cursor(&self, _: &App) -> Option<Point<Pixels>> {
        None
    }

    // TODO
    fn as_searchable(&self, _: &Entity<Self>, _: &App) -> Option<Box<dyn SearchableItemHandle>> {
        None
    }

    fn set_nav_history(
        &mut self,
        _: workspace::ItemNavHistory,
        _window: &mut Window,
        _: &mut Context<Self>,
    ) {
        // TODO
    }

    fn can_save(&self, _cx: &App) -> bool {
        true
    }

    fn save(
        &mut self,
        _options: SaveOptions,
        project: Entity<Project>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        let notebook = self.to_notebook(cx);
        let path = self.notebook_item.read(cx).path.clone();
        let fs = project.read(cx).fs().clone();

        self.mark_as_saved(cx);

        cx.spawn(async move |_this, _cx| {
            let json =
                serde_json::to_string_pretty(&notebook).context("Failed to serialize notebook")?;
            fs.atomic_write(path, json).await?;
            Ok(())
        })
    }

    fn save_as(
        &mut self,
        project: Entity<Project>,
        path: ProjectPath,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        let notebook = self.to_notebook(cx);
        let fs = project.read(cx).fs().clone();

        let abs_path = project.read(cx).absolute_path(&path, cx);

        self.mark_as_saved(cx);

        cx.spawn(async move |_this, _cx| {
            let abs_path = abs_path.context("Failed to get absolute path")?;
            let json =
                serde_json::to_string_pretty(&notebook).context("Failed to serialize notebook")?;
            fs.atomic_write(abs_path, json).await?;
            Ok(())
        })
    }

    fn reload(
        &mut self,
        project: Entity<Project>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        let project_path = self.notebook_item.read(cx).project_path.clone();
        let languages = self.languages.clone();
        let notebook_language = self.notebook_language.clone();

        cx.spawn_in(window, async move |this, cx| {
            let buffer = this
                .update(cx, |this, cx| {
                    this.project
                        .update(cx, |project, cx| project.open_buffer(project_path, cx))
                })?
                .await?;

            let file_content = buffer.read_with(cx, |buffer, _| buffer.text());

            let mut json: serde_json::Value = serde_json::from_str(&file_content)?;
            if let Some(cells) = json.get_mut("cells").and_then(|c| c.as_array_mut()) {
                for cell in cells {
                    if cell.get("id").is_none() {
                        cell["id"] = serde_json::Value::String(Uuid::new_v4().to_string());
                    }
                }
            }
            let file_content = serde_json::to_string(&json)?;

            let notebook = nbformat::parse_notebook(&file_content);
            let notebook = match notebook {
                Ok(nbformat::Notebook::V4(notebook)) => notebook,
                Ok(nbformat::Notebook::Legacy(legacy_notebook)) => {
                    nbformat::upgrade_legacy_notebook(legacy_notebook)?
                }
                Ok(nbformat::Notebook::V3(v3_notebook)) => {
                    nbformat::upgrade_v3_notebook(v3_notebook)?
                }
                Err(e) => {
                    anyhow::bail!("Failed to parse notebook: {:?}", e);
                }
            };

            this.update_in(cx, |this, window, cx| {
                let mut cell_order = vec![];
                let mut cell_map = HashMap::default();

                for cell in notebook.cells.iter() {
                    let cell_id = cell.id();
                    cell_order.push(cell_id.clone());
                    let cell_entity =
                        Cell::load(cell, &languages, notebook_language.clone(), window, cx);
                    cell_map.insert(cell_id.clone(), cell_entity);
                }

                this.cell_order = cell_order.clone();
                this.original_cell_order = cell_order;
                this.cell_map = cell_map;
                this.cell_list =
                    ListState::new(this.cell_order.len(), gpui::ListAlignment::Top, px(1000.));
                cx.notify();
            })?;

            Ok(())
        })
    }

    fn is_dirty(&self, cx: &App) -> bool {
        self.has_structural_changes() || self.has_content_changes(cx)
    }
}

impl ProjectItem for NotebookEditor {
    type Item = NotebookItem;

    fn for_project_item(
        project: Entity<Project>,
        _pane: Option<&Pane>,
        item: Entity<Self::Item>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::new(project, item, window, cx)
    }
}
