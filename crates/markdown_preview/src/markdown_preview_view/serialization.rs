use super::*;

impl SerializableItem for MarkdownPreviewView {
    fn serialized_item_kind() -> &'static str {
        "MarkdownPreviewView"
    }

    fn deserialize(
        project: Entity<Project>,
        workspace: WeakEntity<Workspace>,
        workspace_id: WorkspaceId,
        item_id: ItemId,
        window: &mut Window,
        cx: &mut App,
    ) -> Task<Result<Entity<Self>>> {
        let db = persistence::MarkdownPreviewDb::global(cx);
        window.spawn(cx, async move |cx| {
            let (abs_path, mode_value) = db
                .get_preview(item_id, workspace_id)?
                .context("No markdown preview entry found")?;
            let mode = MarkdownPreviewMode::from_db(mode_value);

            let (worktree, relative_path) = project
                .update(cx, |project, cx| {
                    project.find_or_create_worktree(abs_path.clone(), false, cx)
                })
                .await
                .context("Path not found")?;
            let worktree_id = worktree.read_with(cx, |worktree, _| worktree.id());

            let project_path = ProjectPath {
                worktree_id,
                path: relative_path,
            };

            let buffer = project
                .update(cx, |project, cx| project.open_buffer(project_path, cx))
                .await?;

            cx.update(|window, cx| {
                let language_registry = project.read(cx).languages().clone();
                let editor =
                    cx.new(|cx| Editor::for_buffer(buffer, Some(project.clone()), window, cx));
                MarkdownPreviewView::new(mode, editor, workspace, language_registry, window, cx)
            })
        })
    }

    fn cleanup(
        workspace_id: WorkspaceId,
        alive_items: Vec<ItemId>,
        _window: &mut Window,
        cx: &mut App,
    ) -> Task<Result<()>> {
        let db = persistence::MarkdownPreviewDb::global(cx);
        delete_unloaded_items(alive_items, workspace_id, "markdown_previews", &db, cx)
    }

    fn serialize(
        &mut self,
        workspace: &mut Workspace,
        item_id: ItemId,
        _closing: bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Task<Result<()>>> {
        let workspace_id = workspace.database_id()?;
        let editor = self.active_editor.as_ref()?.editor.clone();
        let buffer = editor.read(cx).buffer().read(cx).as_singleton()?;
        let file = buffer.read(cx).file()?;
        let worktree_id = file.worktree_id(cx);
        let abs_path = workspace
            .project()
            .read(cx)
            .worktree_for_id(worktree_id, cx)?
            .read(cx)
            .absolutize(file.path());
        let mode = self.mode.to_db();
        let db = persistence::MarkdownPreviewDb::global(cx);
        Some(cx.background_spawn(async move {
            db.save_preview(item_id, workspace_id, abs_path, mode).await
        }))
    }

    fn should_serialize(&self, event: &Self::Event) -> bool {
        matches!(
            event,
            MarkdownPreviewEvent::SourceEditorChanged
                | MarkdownPreviewEvent::SourceFileHandleChanged
        )
    }
}
