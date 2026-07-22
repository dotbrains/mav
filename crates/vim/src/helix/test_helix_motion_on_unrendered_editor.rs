use super::*;

// Regression test for MAV-758: helix motions called
// `Editor::text_layout_details` on an editor whose `style` had never
// been set, panicking on `unwrap()`.
#[gpui::test]
async fn test_helix_motion_on_unrendered_editor(cx: &mut gpui::TestAppContext) {
    use editor::{Editor, EditorMode, SelectionEffects};
    use multi_buffer::{MultiBuffer, MultiBufferOffset};

    VimTestContext::init(cx);
    cx.update(|cx| {
        VimTestContext::init_keybindings(true, cx);
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |s| {
                s.vim_mode = Some(true);
                s.helix_mode = Some(true);
            });
        });
    });

    let cx = cx.add_empty_window();

    let editor = cx.update(|window, cx| {
        use gpui::AppContext as _;
        let buffer = MultiBuffer::build_simple("one two three", cx);
        cx.new(|cx| {
            let mut editor = Editor::new(EditorMode::full(), buffer, None, window, cx);
            editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
                s.select_ranges([MultiBufferOffset(4)..MultiBufferOffset(4)])
            });
            editor
        })
    });

    let vim = editor
        .read_with(cx, |editor, _| editor.addon::<VimAddon>().cloned())
        .expect("VimAddon should be auto-attached to new editors when vim mode is enabled");

    cx.update(|window, cx| {
        vim.entity.update(cx, |vim, cx| {
            vim.switch_mode(Mode::HelixNormal, true, window, cx);
            vim.helix_move_and_collapse(crate::motion::Motion::Left, None, window, cx);
        });
    });

    let cursor_offset = cx.update(|_, cx| {
        editor.update(cx, |editor, cx| {
            editor
                .selections
                .newest::<MultiBufferOffset>(&editor.display_snapshot(cx))
                .head()
        })
    });
    assert_eq!(cursor_offset, MultiBufferOffset(3));
}
