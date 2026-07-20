use super::*;

impl Editor {
    pub fn single_line(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let buffer = cx.new(|cx| Buffer::local("", cx));
        let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
        Self::new(EditorMode::SingleLine, buffer, None, window, cx)
    }

    pub fn multi_line(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let buffer = cx.new(|cx| Buffer::local("", cx));
        let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
        Self::new(EditorMode::full(), buffer, None, window, cx)
    }

    pub fn auto_height(
        min_lines: usize,
        max_lines: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let buffer = cx.new(|cx| Buffer::local("", cx));
        let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
        Self::new(
            EditorMode::AutoHeight {
                min_lines,
                max_lines: Some(max_lines),
            },
            buffer,
            None,
            window,
            cx,
        )
    }

    /// Creates a new auto-height editor with a minimum number of lines but no maximum.
    /// The editor grows as tall as needed to fit its content.
    pub fn auto_height_unbounded(
        min_lines: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let buffer = cx.new(|cx| Buffer::local("", cx));
        let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
        Self::new(
            EditorMode::AutoHeight {
                min_lines,
                max_lines: None,
            },
            buffer,
            None,
            window,
            cx,
        )
    }

    pub fn for_buffer(
        buffer: Entity<Buffer>,
        project: Option<Entity<Project>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
        Self::new(EditorMode::full(), buffer, project, window, cx)
    }

    pub fn for_multibuffer(
        buffer: Entity<MultiBuffer>,
        project: Option<Entity<Project>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::new(EditorMode::full(), buffer, project, window, cx)
    }

    pub fn clone(&self, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut clone = Self::new(
            self.mode.clone(),
            self.buffer.clone(),
            self.project.clone(),
            window,
            cx,
        );
        let my_snapshot = self.display_map.update(cx, |display_map, cx| {
            let snapshot = display_map.snapshot(cx);
            clone.display_map.update(cx, |display_map, cx| {
                display_map.set_state(&snapshot, cx);
            });
            snapshot
        });
        let clone_snapshot = clone.display_map.update(cx, |map, cx| map.snapshot(cx));
        clone.folds_did_change(cx);
        clone.selections.clone_state(&self.selections);
        clone
            .scroll_manager
            .clone_state(&self.scroll_manager, &my_snapshot, &clone_snapshot, cx);
        clone.searchable = self.searchable;
        clone.read_only = self.read_only;
        clone.buffers_with_disabled_indent_guides =
            self.buffers_with_disabled_indent_guides.clone();
        clone.enable_mouse_wheel_zoom = self.enable_mouse_wheel_zoom;
        clone.enable_lsp_data = self.enable_lsp_data;
        clone.needs_initial_data_update = self.enable_lsp_data;
        clone.enable_runnables = self.enable_runnables;
        clone.enable_code_lens = self.enable_code_lens;
        clone
    }

    pub fn new(
        mode: EditorMode,
        buffer: Entity<MultiBuffer>,
        project: Option<Entity<Project>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Editor::new_internal(mode, buffer, project, None, window, cx)
    }
}
