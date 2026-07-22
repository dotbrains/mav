
mod selection;
use std::{ops::Range, time::Duration};

use super::*;
use editor::{
    DisplayPoint, Editor, HighlightKey, MultiBuffer, PathKey, SELECTION_HIGHLIGHT_DEBOUNCE_TIMEOUT,
    SearchSettings, SelectionEffects, display_map::DisplayRow,
    test::editor_test_context::EditorTestContext,
};
use futures::stream::StreamExt as _;
use gpui::{Hsla, TestAppContext, UpdateGlobal, VisualTestContext};
use language::{Buffer, Point};
#[cfg(target_os = "macos")]
use project::Project;
use settings::{SearchSettingsContent, SettingsStore};
use unindent::Unindent as _;
use util_macros::perf;
#[cfg(target_os = "macos")]
use workspace::{AppState, MultiWorkspace, Workspace};

fn init_globals(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let store = settings::SettingsStore::test(cx);
        cx.set_global(store);
        editor::init(cx);

        theme_settings::init(theme::LoadThemes::JustBase, cx);
        crate::init(cx);
    });
}

fn init_multibuffer_test(
    cx: &mut TestAppContext,
) -> (
    Entity<Editor>,
    Entity<BufferSearchBar>,
    &mut VisualTestContext,
) {
    init_globals(cx);

    let buffer1 = cx.new(|cx| {
            Buffer::local(
                            r#"
                            A regular expression (shortened as regex or regexp;[1] also referred to as
                            rational expression[2][3]) is a sequence of characters that specifies a search
                            pattern in text. Usually such patterns are used by string-searching algorithms
                            for "find" or "find and replace" operations on strings, or for input validation.
                            "#
                            .unindent(),
                            cx,
                        )
        });

    let buffer2 = cx.new(|cx| {
        Buffer::local(
            r#"
                            Some Additional text with the term regular expression in it.
                            There two lines.
                            "#
            .unindent(),
            cx,
        )
    });

    let multibuffer = cx.new(|cx| {
        let mut buffer = MultiBuffer::new(language::Capability::ReadWrite);

        //[ExcerptRange::new(Point::new(0, 0)..Point::new(2, 0))]
        buffer.set_excerpts_for_path(
            PathKey::sorted(0),
            buffer1,
            [Point::new(0, 0)..Point::new(3, 0)],
            0,
            cx,
        );
        buffer.set_excerpts_for_path(
            PathKey::sorted(1),
            buffer2,
            [Point::new(0, 0)..Point::new(1, 0)],
            0,
            cx,
        );

        buffer
    });
    let mut editor = None;
    let window = cx.add_window(|window, cx| {
        let default_key_bindings = settings::KeymapFile::load_asset_allow_partial_failure(
            "keymaps/default-macos.json",
            cx,
        )
        .unwrap();
        cx.bind_keys(default_key_bindings);
        editor = Some(cx.new(|cx| Editor::for_multibuffer(multibuffer.clone(), None, window, cx)));

        let mut search_bar = BufferSearchBar::new(None, window, cx);
        search_bar.set_active_pane_item(Some(&editor.clone().unwrap()), window, cx);
        search_bar.show(window, cx);
        search_bar
    });
    let search_bar = window.root(cx).unwrap();

    let cx = VisualTestContext::from_window(*window, cx).into_mut();

    (editor.unwrap(), search_bar, cx)
}

fn init_test(
    cx: &mut TestAppContext,
) -> (
    Entity<Editor>,
    Entity<BufferSearchBar>,
    &mut VisualTestContext,
) {
    init_globals(cx);
    let buffer = cx.new(|cx| {
        Buffer::local(
            r#"
                A regular expression (shortened as regex or regexp;[1] also referred to as
                rational expression[2][3]) is a sequence of characters that specifies a search
                pattern in text. Usually such patterns are used by string-searching algorithms
                for "find" or "find and replace" operations on strings, or for input validation.
                "#
            .unindent(),
            cx,
        )
    });
    let mut editor = None;
    let window = cx.add_window(|window, cx| {
        let default_key_bindings = settings::KeymapFile::load_asset_allow_partial_failure(
            "keymaps/default-macos.json",
            cx,
        )
        .unwrap();
        cx.bind_keys(default_key_bindings);
        editor = Some(cx.new(|cx| Editor::for_buffer(buffer.clone(), None, window, cx)));
        let mut search_bar = BufferSearchBar::new(None, window, cx);
        search_bar.set_active_pane_item(Some(&editor.clone().unwrap()), window, cx);
        search_bar.show(window, cx);
        search_bar
    });
    let search_bar = window.root(cx).unwrap();

    let cx = VisualTestContext::from_window(*window, cx).into_mut();

    (editor.unwrap(), search_bar, cx)
}

fn display_points_of(
    background_highlights: Vec<(Range<DisplayPoint>, Hsla)>,
) -> Vec<Range<DisplayPoint>> {
    background_highlights
        .into_iter()
        .map(|(range, _)| range)
        .collect::<Vec<_>>()
}

mod history;
mod option_handling;
mod options;
mod replace;
mod search_flow;
mod selection_matches;
mod workspace_layout;

fn update_search_settings(search_settings: SearchSettings, cx: &mut TestAppContext) {
    cx.update(|cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.editor.search = Some(SearchSettingsContent {
                    button: Some(search_settings.button),
                    whole_word: Some(search_settings.whole_word),
                    case_sensitive: Some(search_settings.case_sensitive),
                    include_ignored: Some(search_settings.include_ignored),
                    regex: Some(search_settings.regex),
                    center_on_match: Some(search_settings.center_on_match),
                });
            });
        });
    });
}
