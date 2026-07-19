use super::*;
use editor::Editor;
use gpui::{TestAppContext, VisualTestContext};
use language::{Language, LanguageConfig};
use project::{Project, ProjectPath};
use serde_json::json;
use std::sync::Arc;
use util::{path, rel_path::rel_path};
use workspace::{AppState, MultiWorkspace, Workspace};

fn init_test(cx: &mut TestAppContext) -> Arc<AppState> {
    cx.update(|cx| {
        let app_state = AppState::test(cx);
        settings::init(cx);
        super::init(cx);
        editor::init(cx);
        app_state
    })
}

fn register_test_languages(project: &Entity<Project>, cx: &mut VisualTestContext) {
    project.read_with(cx, |project, _| {
        let language_registry = project.languages();
        for (language_name, path_suffix) in [
            ("C", "c"),
            ("Go", "go"),
            ("Ruby", "rb"),
            ("Rust", "rs"),
            ("TypeScript", "ts"),
        ] {
            language_registry.add(Arc::new(Language::new(
                LanguageConfig {
                    name: language_name.into(),
                    matcher: LanguageMatcher {
                        path_suffixes: vec![path_suffix.to_string()],
                        ..Default::default()
                    },
                    ..Default::default()
                },
                None,
            )));
        }
    });
}

async fn open_file_editor(
    workspace: &Entity<Workspace>,
    project: &Entity<Project>,
    file_path: &str,
    cx: &mut VisualTestContext,
) -> Entity<Editor> {
    let worktree_id = project.update(cx, |project, cx| {
        project
            .worktrees(cx)
            .next()
            .expect("project should have a worktree")
            .read(cx)
            .id()
    });
    let project_path = ProjectPath {
        worktree_id,
        path: rel_path(file_path).into(),
    };
    let opened_item = workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_path(project_path, None, true, window, cx)
        })
        .await
        .expect("file should open");

    cx.update(|_, cx| {
        opened_item
            .act_as::<Editor>(cx)
            .expect("opened item should be an editor")
    })
}

async fn open_empty_editor(
    workspace: &Entity<Workspace>,
    project: &Entity<Project>,
    cx: &mut VisualTestContext,
) -> Entity<Editor> {
    let editor = open_new_buffer_editor(workspace, project, cx).await;
    let buffer = editor.read_with(cx, |editor, cx| {
        editor
            .active_buffer(cx)
            .expect("editor should have an active buffer")
    });
    buffer.update(cx, |buffer, cx| {
        buffer.set_language(None, cx);
    });
    editor
}

async fn open_new_buffer_editor(
    workspace: &Entity<Workspace>,
    project: &Entity<Project>,
    cx: &mut VisualTestContext,
) -> Entity<Editor> {
    let create_buffer = project.update(cx, |project, cx| project.create_buffer(None, true, cx));
    let buffer = create_buffer.await.expect("empty buffer should be created");
    let editor = cx.new_window_entity(|window, cx| {
        Editor::for_buffer(buffer.clone(), Some(project.clone()), window, cx)
    });
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.add_item_to_center(Box::new(editor.clone()), window, cx);
    });
    editor
}

async fn set_editor_language(
    project: &Entity<Project>,
    editor: &Entity<Editor>,
    language_name: &str,
    cx: &mut VisualTestContext,
) {
    let language = project
        .read_with(cx, |project, _| {
            project.languages().language_for_name(language_name)
        })
        .await
        .expect("language should exist in registry");
    editor.update(cx, move |editor, cx| {
        let buffer = editor
            .active_buffer(cx)
            .expect("editor should have an active excerpt");
        buffer.update(cx, |buffer, cx| {
            buffer.set_language(Some(language), cx);
        });
    });
}

fn active_picker(
    workspace: &Entity<Workspace>,
    cx: &mut VisualTestContext,
) -> Entity<Picker<LanguageSelectorDelegate>> {
    workspace.update(cx, |workspace, cx| {
        workspace
            .active_modal::<LanguageSelector>(cx)
            .expect("language selector should be open")
            .read(cx)
            .picker
            .clone()
    })
}

fn open_selector(
    workspace: &Entity<Workspace>,
    cx: &mut VisualTestContext,
) -> Entity<Picker<LanguageSelectorDelegate>> {
    cx.dispatch_action(Toggle);
    cx.run_until_parked();
    active_picker(workspace, cx)
}

fn close_selector(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) {
    cx.dispatch_action(Toggle);
    cx.run_until_parked();
    workspace.read_with(cx, |workspace, cx| {
        assert!(
            workspace.active_modal::<LanguageSelector>(cx).is_none(),
            "language selector should be closed"
        );
    });
}

fn assert_selected_language_for_editor(
    workspace: &Entity<Workspace>,
    editor: &Entity<Editor>,
    expected_language_name: Option<&str>,
    cx: &mut VisualTestContext,
) {
    workspace.update_in(cx, |workspace, window, cx| {
        let was_activated = workspace.activate_item(editor, true, true, window, cx);
        assert!(
            was_activated,
            "editor should be activated before opening the modal"
        );
    });
    cx.run_until_parked();

    let picker = open_selector(workspace, cx);
    picker.read_with(cx, |picker, _| {
        let selected_match = picker
            .delegate
            .matches
            .get(picker.delegate.selected_index)
            .expect("selected index should point to a match");
        let selected_candidate = picker
            .delegate
            .candidates
            .get(selected_match.candidate_id)
            .expect("selected match should map to a candidate");

        if let Some(expected_language_name) = expected_language_name {
            let current_language_candidate_index = picker
                .delegate
                .current_language_candidate_index
                .expect("current language should map to a candidate");
            assert_eq!(
                selected_match.candidate_id,
                current_language_candidate_index
            );
            assert_eq!(selected_candidate.string, expected_language_name);
        } else {
            assert!(picker.delegate.current_language_candidate_index.is_none());
            assert_eq!(picker.delegate.selected_index, 0);
        }
    });
    close_selector(workspace, cx);
}

#[gpui::test]
async fn test_language_selector_selects_current_language_per_active_editor(
    cx: &mut TestAppContext,
) {
    let app_state = init_test(cx);
    app_state
        .fs
        .as_fake()
        .insert_tree(
            path!("/test"),
            json!({
                "rust_file.rs": "fn main() {}\n",
                "typescript_file.ts": "const value = 1;\n",
            }),
        )
        .await;

    let project = Project::test(app_state.fs.clone(), [path!("/test").as_ref()], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace =
        multi_workspace.read_with(cx, |multi_workspace, _| multi_workspace.workspace().clone());
    register_test_languages(&project, cx);

    let rust_editor = open_file_editor(&workspace, &project, "rust_file.rs", cx).await;
    let typescript_editor = open_file_editor(&workspace, &project, "typescript_file.ts", cx).await;
    let empty_editor = open_empty_editor(&workspace, &project, cx).await;

    set_editor_language(&project, &rust_editor, "Rust", cx).await;
    set_editor_language(&project, &typescript_editor, "TypeScript", cx).await;
    cx.run_until_parked();

    assert_selected_language_for_editor(&workspace, &rust_editor, Some("Rust"), cx);
    assert_selected_language_for_editor(&workspace, &typescript_editor, Some("TypeScript"), cx);
    let buffer = empty_editor.read_with(cx, |editor, cx| {
        editor
            .active_buffer(cx)
            .expect("editor should have an active excerpt")
    });
    buffer.update(cx, |buffer, cx| {
        buffer.set_language(None, cx);
    });
    assert_selected_language_for_editor(&workspace, &empty_editor, None, cx);
}

#[gpui::test]
async fn test_language_selector_selects_first_match_after_querying_new_buffer(
    cx: &mut TestAppContext,
) {
    let app_state = init_test(cx);
    app_state
        .fs
        .as_fake()
        .insert_tree(path!("/test"), json!({}))
        .await;

    let project = Project::test(app_state.fs.clone(), [path!("/test").as_ref()], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace =
        multi_workspace.read_with(cx, |multi_workspace, _| multi_workspace.workspace().clone());
    register_test_languages(&project, cx);

    let editor = open_new_buffer_editor(&workspace, &project, cx).await;
    workspace.update_in(cx, |workspace, window, cx| {
        let was_activated = workspace.activate_item(&editor, true, true, window, cx);
        assert!(
            was_activated,
            "editor should be activated before opening the modal"
        );
    });
    cx.run_until_parked();

    let picker = open_selector(&workspace, cx);
    picker.read_with(cx, |picker, _| {
        let selected_match = picker
            .delegate
            .matches
            .get(picker.delegate.selected_index)
            .expect("selected index should point to a match");
        let selected_candidate = picker
            .delegate
            .candidates
            .get(selected_match.candidate_id)
            .expect("selected match should map to a candidate");

        assert_eq!(selected_candidate.string, "Plain Text");
        assert!(
            picker
                .delegate
                .current_language_candidate_index
                .is_some_and(|current_language_candidate_index| {
                    current_language_candidate_index > 1
                }),
            "test setup should place Plain Text after at least two earlier languages",
        );
    });

    picker.update_in(cx, |picker, window, cx| {
        picker.update_matches("ru".to_string(), window, cx)
    });
    cx.run_until_parked();

    picker.read_with(cx, |picker, _| {
        assert!(
            picker.delegate.matches.len() > 1,
            "query should return multiple matches"
        );
        assert_eq!(picker.delegate.selected_index, 0);

        let first_match = picker
            .delegate
            .matches
            .first()
            .expect("query should produce at least one match");
        let selected_match = picker
            .delegate
            .matches
            .get(picker.delegate.selected_index)
            .expect("selected index should point to a match");

        assert_eq!(selected_match.candidate_id, first_match.candidate_id);
    });
}
