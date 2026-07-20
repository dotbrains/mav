use super::*;

/// Addons allow storing per-editor state in other crates (e.g. Vim)
pub trait Addon: 'static {
    fn extend_key_context(&self, _: &mut KeyContext, _: &App) {}

    fn render_buffer_header_controls(
        &self,
        _: &ExcerptBoundaryInfo,
        _: &language::BufferSnapshot,
        _: &Window,
        _: &App,
    ) -> Option<AnyElement> {
        None
    }

    fn extend_buffer_header_context_menu(
        &self,
        menu: ui::ContextMenu,
        _: &language::BufferSnapshot,
        _: &mut Window,
        _: &mut App,
    ) -> ui::ContextMenu {
        menu
    }

    fn override_status_for_buffer_id(&self, _: BufferId, _: &App) -> Option<FileStatus> {
        None
    }

    fn to_any(&self) -> &dyn std::any::Any;

    fn to_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        None
    }
}
