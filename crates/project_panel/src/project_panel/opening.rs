use super::*;

impl ProjectPanel {
    pub(super) fn open(&mut self, _: &Open, window: &mut Window, cx: &mut Context<Self>) {
        let preview_tabs_enabled =
            PreviewTabsSettings::get_global(cx).enable_preview_from_project_panel;
        self.open_internal(true, !preview_tabs_enabled, None, window, cx);
    }

    pub(super) fn open_permanent(
        &mut self,
        _: &OpenPermanent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_internal(false, true, None, window, cx);
    }

    pub(super) fn open_split_vertical(
        &mut self,
        _: &OpenSplitVertical,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_internal(false, true, Some(SplitDirection::vertical(cx)), window, cx);
    }

    pub(super) fn open_split_horizontal(
        &mut self,
        _: &OpenSplitHorizontal,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_internal(
            false,
            true,
            Some(SplitDirection::horizontal(cx)),
            window,
            cx,
        );
    }

    pub(super) fn open_markdown_preview(
        &mut self,
        _: &OpenMarkdownPreview,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((worktree, entry)) = self.selected_entry(cx) else {
            return;
        };
        if !entry.is_file() || !MarkdownPreviewView::is_markdown_path(&*entry.path) {
            return;
        }
        let project_path = ProjectPath {
            worktree_id: worktree.id(),
            path: entry.path.clone(),
        };
        self.workspace
            .update(cx, |workspace, cx| {
                MarkdownPreviewView::open_for_project_path(project_path, workspace, window, cx);
            })
            .ok();
    }

    pub(super) fn open_internal(
        &mut self,
        allow_preview: bool,
        focus_opened_item: bool,
        split_direction: Option<SplitDirection>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some((_, entry)) = self.selected_entry(cx) {
            if entry.is_file() {
                if split_direction.is_some() {
                    self.split_entry(entry.id, allow_preview, split_direction, cx);
                } else {
                    self.open_entry(entry.id, focus_opened_item, allow_preview, cx);
                }
                cx.notify();
            } else {
                self.toggle_expanded(entry.id, window, cx);
            }
        }
    }
}
