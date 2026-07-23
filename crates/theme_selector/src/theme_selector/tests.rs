use super::*;
use gpui::{TestAppContext, VisualTestContext};
use project::Project;
use serde_json::json;
use theme::{Appearance, ThemeFamily, ThemeRegistry, default_color_scales};
use util::path;
use workspace::MultiWorkspace;

fn init_test(cx: &mut TestAppContext) -> Arc<workspace::AppState> {
    cx.update(|cx| {
        let app_state = workspace::AppState::test(cx);
        settings::init(cx);
        theme::init(theme::LoadThemes::JustBase, cx);
        editor::init(cx);
        super::init(cx);
        app_state
    })
}

fn register_test_themes(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let registry = ThemeRegistry::global(cx);
        let base_theme = registry.get("One Dark").unwrap();

        let mut test_light = (*base_theme).clone();
        test_light.id = "test-light".to_string();
        test_light.name = "Test Light".into();
        test_light.appearance = Appearance::Light;

        let mut test_dark_a = (*base_theme).clone();
        test_dark_a.id = "test-dark-a".to_string();
        test_dark_a.name = "Test Dark A".into();

        let mut test_dark_b = (*base_theme).clone();
        test_dark_b.id = "test-dark-b".to_string();
        test_dark_b.name = "Test Dark B".into();

        registry.register_test_themes([ThemeFamily {
            id: "test-family".to_string(),
            name: "Test Family".into(),
            author: "test".into(),
            themes: vec![test_light, test_dark_a, test_dark_b],
            scales: default_color_scales(),
        }]);
    });
}

async fn setup_test(cx: &mut TestAppContext) -> Arc<workspace::AppState> {
    let app_state = init_test(cx);
    register_test_themes(cx);
    app_state
        .fs
        .as_fake()
        .insert_tree(path!("/test"), json!({}))
        .await;
    app_state
}

fn open_theme_selector(
    workspace: &Entity<workspace::Workspace>,
    cx: &mut VisualTestContext,
) -> Entity<Picker<ThemeSelectorDelegate>> {
    cx.dispatch_action(mav_actions::theme_selector::Toggle {
        themes_filter: None,
    });
    cx.run_until_parked();
    workspace.update(cx, |workspace, cx| {
        workspace
            .active_modal::<ThemeSelector>(cx)
            .expect("theme selector should be open")
            .read(cx)
            .picker
            .clone()
    })
}

fn selected_theme_name(
    picker: &Entity<Picker<ThemeSelectorDelegate>>,
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
    picker: &Entity<Picker<ThemeSelectorDelegate>>,
    cx: &mut VisualTestContext,
) -> String {
    picker.read_with(cx, |picker, _| picker.delegate.new_theme.name.to_string())
}

#[gpui::test]
async fn test_theme_selector_preserves_selection_on_empty_filter(cx: &mut TestAppContext) {
    let app_state = setup_test(cx).await;
    let project = Project::test(app_state.fs.clone(), [path!("/test").as_ref()], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace =
        multi_workspace.read_with(cx, |multi_workspace, _| multi_workspace.workspace().clone());
    let picker = open_theme_selector(&workspace, cx);

    let target_index = picker.read_with(cx, |picker, _| {
        picker
            .delegate
            .matches
            .iter()
            .position(|m| m.string == "Test Light")
            .unwrap()
    });
    picker.update_in(cx, |picker, window, cx| {
        picker.set_selected_index(target_index, None, true, window, cx);
    });
    cx.run_until_parked();

    assert_eq!(previewed_theme_name(&picker, cx), "Test Light");

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
        "Test Light",
        "selected theme should be preserved after clearing an empty filter"
    );
    assert_eq!(
        previewed_theme_name(&picker, cx),
        "Test Light",
        "previewed theme should be preserved after clearing an empty filter"
    );
}
