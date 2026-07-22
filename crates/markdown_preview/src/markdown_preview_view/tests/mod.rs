use super::super::*;
use crate::markdown_preview_view::ImageSource;
use crate::markdown_preview_view::Resource;
use crate::markdown_preview_view::resolve_preview_image;
use buffer_diff::BufferDiff;
use editor::Editor;
use gpui::{AppContext as _, Entity, TestAppContext};
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use util::path;
use util::rel_path::{RelPath, rel_path};
use util::test::TempTree;
use workspace::item::SerializableItem;
use workspace::{AppState, ItemId, MultiWorkspace, SaveIntent, Workspace, WorkspaceId, open_paths};
fn init_test(cx: &mut TestAppContext) -> Arc<AppState> {
    cx.update(|cx| {
        let state = AppState::test(cx);
        editor::init(cx);
        crate::init(cx);
        state
    })
}

async fn wait_for_preview_serialization(cx: &mut TestAppContext) {
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(250));
    cx.run_until_parked();
}

fn saved_preview_path(
    cx: &mut TestAppContext,
    item_id: ItemId,
    workspace_id: WorkspaceId,
) -> PathBuf {
    cx.update(|cx| {
        super::persistence::MarkdownPreviewDb::global(cx)
            .get_preview(item_id, workspace_id)
            .unwrap()
            .unwrap()
            .0
    })
}

fn preview_source_path(
    cx: &mut TestAppContext,
    preview: &Entity<MarkdownPreviewView>,
) -> Arc<RelPath> {
    let editor = preview.read_with(cx, |preview, _| {
        preview.active_editor.as_ref().unwrap().editor.clone()
    });
    editor_source_path(cx, &editor)
}

fn editor_source_path(cx: &mut TestAppContext, editor: &Entity<Editor>) -> Arc<RelPath> {
    editor.read_with(cx, |editor, cx| {
        let buffer = editor.buffer().read(cx).as_singleton().unwrap();
        buffer.read(cx).file().unwrap().path().clone()
    })
}

fn markdown_fixture_directory(tree: &TempTree) -> PathBuf {
    tree.path().join("docs")
}

#[track_caller]
fn assert_resolved_preview_image_path(
    resolved: Option<ImageSource>,
    expected_path: &std::path::Path,
) {
    match resolved {
        Some(ImageSource::Resource(Resource::Path(path))) => {
            assert_eq!(path.as_ref(), expected_path);
        }
        _ => panic!("Expected preview image to resolve to a local path"),
    }
}

mod binding_tests;
mod editor_tests;
mod image_tests;
mod serialization_tests;
