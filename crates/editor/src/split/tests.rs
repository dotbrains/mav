
use std::{any::TypeId, sync::Arc};

use buffer_diff::BufferDiff;
use collections::{HashMap, HashSet};
use fs::FakeFs;
use gpui::{AppContext as _, Entity, Pixels, VisualTestContext};
use gpui::{BorrowAppContext as _, Element as _};
use language::language_settings::SoftWrap;
use language::{Buffer, Capability};
use multi_buffer::{MultiBuffer, PathKey};
use pretty_assertions::assert_eq;
use project::Project;
use rand::rngs::StdRng;
use settings::{DiffViewStyle, SettingsStore};
use ui::{VisualContext as _, div, px};
use util::rel_path::rel_path;
use workspace::{Item, MultiWorkspace};

use crate::display_map::{BlockPlacement, BlockProperties, BlockStyle, Crease, FoldPlaceholder};
use crate::inlays::Inlay;
use crate::test::{editor_content_with_blocks_and_width, set_block_content_for_tests};
use crate::{Editor, SplittableEditor};
use multi_buffer::MultiBufferOffset;

mod basic;
mod random;

async fn init_test(
    cx: &mut gpui::TestAppContext,
    soft_wrap: SoftWrap,
    style: DiffViewStyle,
) -> (Entity<SplittableEditor>, &mut VisualTestContext) {
    cx.update(|cx| {
        let store = SettingsStore::test(cx);
        cx.set_global(store);
        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.editor.diff_view_style = Some(style);
            });
        });
        theme_settings::init(theme::LoadThemes::JustBase, cx);
        crate::init(cx);
    });
    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs as Arc<dyn fs::Fs>, [], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());
    let rhs_multibuffer = cx.new(|cx| {
        let mut multibuffer = MultiBuffer::new(Capability::ReadWrite);
        multibuffer.set_all_diff_hunks_expanded(cx);
        multibuffer
    });
    let editor = cx.new_window_entity(|window, cx| {
        let editor = SplittableEditor::new(
            style,
            rhs_multibuffer.clone(),
            project.clone(),
            workspace,
            window,
            cx,
        );
        editor.update_editors(cx, |editor, cx| {
            editor.set_soft_wrap_mode(soft_wrap, cx);
        });
        editor
    });
    (editor, cx)
}

fn buffer_with_diff(
    base_text: &str,
    current_text: &str,
    cx: &mut VisualTestContext,
) -> (Entity<Buffer>, Entity<BufferDiff>) {
    let buffer = cx.new(|cx| Buffer::local(current_text.to_string(), cx));
    let diff = cx
        .new(|cx| BufferDiff::new_with_base_text(base_text, &buffer.read(cx).text_snapshot(), cx));
    (buffer, diff)
}

#[track_caller]
fn assert_split_content(
    editor: &Entity<SplittableEditor>,
    expected_rhs: String,
    expected_lhs: String,
    cx: &mut VisualTestContext,
) {
    assert_split_content_with_widths(
        editor,
        px(3000.0),
        px(3000.0),
        expected_rhs,
        expected_lhs,
        cx,
    );
}

#[track_caller]
fn assert_split_content_with_widths(
    editor: &Entity<SplittableEditor>,
    rhs_width: Pixels,
    lhs_width: Pixels,
    expected_rhs: String,
    expected_lhs: String,
    cx: &mut VisualTestContext,
) {
    let (rhs_editor, lhs_editor) = editor.update(cx, |editor, _cx| {
        let lhs = editor.lhs.as_ref().expect("should have lhs editor");
        (editor.rhs_editor.clone(), lhs.editor.clone())
    });

    // Make sure both sides learn if the other has soft-wrapped
    let _ = editor_content_with_blocks_and_width(&rhs_editor, rhs_width, cx);
    cx.run_until_parked();
    let _ = editor_content_with_blocks_and_width(&lhs_editor, lhs_width, cx);
    cx.run_until_parked();

    let rhs_content = editor_content_with_blocks_and_width(&rhs_editor, rhs_width, cx);
    let lhs_content = editor_content_with_blocks_and_width(&lhs_editor, lhs_width, cx);

    if rhs_content != expected_rhs || lhs_content != expected_lhs {
        editor.update(cx, |editor, cx| editor.debug_print(cx));
    }

    assert_eq!(rhs_content, expected_rhs, "rhs");
    assert_eq!(lhs_content, expected_lhs, "lhs");
}

mod addition_tests;
mod basic;
mod blank_and_deletion_tests;
mod custom_block_hunk_tests;
mod custom_block_sync_tests;
mod custom_block_unsplit_tests;
mod folding_tests;
mod misc_split_tests;
mod path_and_type_tests;
mod random;
mod scrolling_and_edit_tests;
mod soft_wrap_tests;
