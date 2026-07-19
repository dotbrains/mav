use super::*;
use std::collections::HashMap;

use gpui::{TestAppContext, VisualTestContext};
use project::Project;
use serde_json::json;
use theme::{ChevronIcons, DirectoryIcons, IconTheme, ThemeRegistry};
use util::path;
use workspace::MultiWorkspace;

fn init_test(cx: &mut TestAppContext) -> Arc<workspace::AppState> {
    cx.update(|cx| {
        let app_state = workspace::AppState::test(cx);
        settings::init(cx);
        theme::init(theme::LoadThemes::JustBase, cx);
        editor::init(cx);
        crate::init(cx);
        app_state
    })
}

fn register_test_icon_themes(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let registry = ThemeRegistry::global(cx);
        let make_icon_theme = |name: &str, appearance: Appearance| IconTheme {
            id: name.to_lowercase().replace(' ', "-"),
            name: SharedString::from(name.to_string()),
            appearance,
            directory_icons: DirectoryIcons {
                collapsed: None,
                expanded: None,
            },
            named_directory_icons: HashMap::default(),
            chevron_icons: ChevronIcons {
                collapsed: None,
                expanded: None,
            },
            file_icons: HashMap::default(),
            file_stems: HashMap::default(),
            file_suffixes: HashMap::default(),
        };
        registry.register_test_icon_themes([
            make_icon_theme("Test Icons A", Appearance::Dark),
            make_icon_theme("Test Icons B", Appearance::Dark),
        ]);
    });
}

async fn setup_test(cx: &mut TestAppContext) -> Arc<workspace::AppState> {
    let app_state = init_test(cx);
    register_test_icon_themes(cx);
    app_state
        .fs
        .as_fake()
        .insert_tree(path!("/test"), json!({}))
        .await;
    app_state
}

fn open_icon_theme_selector(
    workspace: &Entity<workspace::Workspace>,
    cx: &mut VisualTestContext,
) -> Entity<Picker<IconThemeSelectorDelegate>> {
    cx.dispatch_action(mav_actions::icon_theme_selector::Toggle {
        themes_filter: None,
    });
    cx.run_until_parked();
    workspace.update(cx, |workspace, cx| {
        workspace
            .active_modal::<IconThemeSelector>(cx)
            .expect("icon theme selector should be open")
            .read(cx)
            .picker
            .clone()
    })
}

fn selected_theme_name(
    picker: &Entity<Picker<IconThemeSelectorDelegate>>,
    cx: &mut VisualTestContext,
) -> String {
    picker.read_with(cx, |picker, _| {
        picker
            .delegate
            .matches
            .get(picker.delegate.selected_index)
            .expect("selected index should point to a match")
            .string
            .clone()
    })
}

fn previewed_theme_name(
    _picker: &Entity<Picker<IconThemeSelectorDelegate>>,
    cx: &mut VisualTestContext,
) -> String {
    cx.read(|cx| {
        ThemeSettings::get_global(cx)
            .icon_theme
            .name(SystemAppearance::global(cx).0)
            .0
            .to_string()
    })
}

#[gpui::test]
async fn test_icon_theme_selector_preserves_selection_on_empty_filter(cx: &mut TestAppContext) {
    let app_state = setup_test(cx).await;
    let project = Project::test(app_state.fs.clone(), [path!("/test").as_ref()], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace =
        multi_workspace.read_with(cx, |multi_workspace, _| multi_workspace.workspace().clone());
    let picker = open_icon_theme_selector(&workspace, cx);

    let target_index = picker.read_with(cx, |picker, _| {
        picker
            .delegate
            .matches
            .iter()
            .position(|m| m.string == "Test Icons A")
            .unwrap()
    });
    picker.update_in(cx, |picker, window, cx| {
        picker.set_selected_index(target_index, None, true, window, cx);
    });
    cx.run_until_parked();

    assert_eq!(previewed_theme_name(&picker, cx), "Test Icons A");

    picker.update_in(cx, |picker, window, cx| {
        picker.update_matches("zzz".to_string(), window, cx);
    });
    cx.run_until_parked();

    picker.update_in(cx, |picker, window, cx| {
        picker.update_matches("".to_string(), window, cx);
    });
    cx.run_until_parked();

    assert_eq!(
        selected_theme_name(&picker, cx),
        "Test Icons A",
        "selected icon theme should be preserved after clearing an empty filter"
    );
    assert_eq!(
        previewed_theme_name(&picker, cx),
        "Test Icons A",
        "previewed icon theme should be preserved after clearing an empty filter"
    );
}
