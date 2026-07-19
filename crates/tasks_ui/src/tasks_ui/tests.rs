use std::{collections::HashMap, sync::Arc};

use editor::{Editor, MultiBufferOffset, SelectionEffects};
use gpui::TestAppContext;
use language::{Language, LanguageConfig};
use project::{BasicContextProvider, FakeFs, Project, task_store::TaskStore};
use serde_json::json;
use task::{TaskContext, TaskVariables, VariableName};
use ui::VisualContext;
use util::{path, rel_path::rel_path};
use workspace::{AppState, MultiWorkspace};

use crate::task_contexts;

#[gpui::test]
async fn test_default_language_context(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/dir"),
        json!({
            ".mav": {
                "tasks.json": r#"[
                        {
                            "label": "example task",
                            "command": "echo",
                            "args": ["4"]
                        },
                        {
                            "label": "another one",
                            "command": "echo",
                            "args": ["55"]
                        },
                    ]"#,
            },
            "a.ts": "function this_is_a_test() { }",
            "rust": {
                                "b.rs": "use std; fn this_is_a_rust_file() { }",
            }

        }),
    )
    .await;
    let project = Project::test(fs, [path!("/dir").as_ref()], cx).await;
    let (worktree_store, git_store) = project.read_with(cx, |project, _| {
        (project.worktree_store(), project.git_store().clone())
    });
    let rust_language = Arc::new(
        Language::new(
            LanguageConfig {
                name: "Rust".into(),
                ..Default::default()
            },
            Some(tree_sitter_rust::LANGUAGE.into()),
        )
        .with_outline_query(
            r#"(function_item
        "fn" @context
        name: (_) @name) @item"#,
        )
        .unwrap()
        .with_context_provider(Some(Arc::new(BasicContextProvider::new(
            worktree_store.clone(),
            git_store.clone(),
        )))),
    );

    let typescript_language = Arc::new(
        Language::new(
            LanguageConfig {
                name: "TypeScript".into(),
                ..Default::default()
            },
            Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
        )
        .with_outline_query(
            r#"(function_declaration
                "async"? @context
                "function" @context
                name: (_) @name
                parameters: (formal_parameters
                    "(" @context
                    ")" @context)) @item"#,
        )
        .unwrap()
        .with_context_provider(Some(Arc::new(BasicContextProvider::new(
            worktree_store.clone(),
            git_store.clone(),
        )))),
    );

    let worktree_id = project.update(cx, |project, cx| {
        project.worktrees(cx).next().unwrap().read(cx).id()
    });
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    let buffer1 = workspace
        .update(cx, |this, cx| {
            this.project().update(cx, |this, cx| {
                this.open_buffer((worktree_id, rel_path("a.ts")), cx)
            })
        })
        .await
        .unwrap();
    buffer1.update(cx, |this, cx| {
        this.set_language(Some(typescript_language), cx)
    });
    let editor1 = cx.new_window_entity(|window, cx| {
        Editor::for_buffer(buffer1, Some(project.clone()), window, cx)
    });
    let buffer2 = workspace
        .update(cx, |this, cx| {
            this.project().update(cx, |this, cx| {
                this.open_buffer((worktree_id, rel_path("rust/b.rs")), cx)
            })
        })
        .await
        .unwrap();
    buffer2.update(cx, |this, cx| this.set_language(Some(rust_language), cx));
    let editor2 =
        cx.new_window_entity(|window, cx| Editor::for_buffer(buffer2, Some(project), window, cx));

    let first_context = workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.add_item_to_center(Box::new(editor1.clone()), window, cx);
            workspace.add_item_to_center(Box::new(editor2.clone()), window, cx);
            assert_eq!(
                workspace.active_item(cx).unwrap().item_id(),
                editor2.entity_id()
            );
            task_contexts(workspace, window, cx)
        })
        .await;

    assert_eq!(
        first_context
            .active_context()
            .expect("Should have an active context"),
        &TaskContext {
            cwd: Some(path!("/dir").into()),
            task_variables: TaskVariables::from_iter([
                (VariableName::File, path!("/dir/rust/b.rs").into()),
                (VariableName::Filename, "b.rs".into()),
                (VariableName::RelativeFile, path!("rust/b.rs").into()),
                (VariableName::RelativeDir, "rust".into()),
                (VariableName::Dirname, path!("/dir/rust").into()),
                (VariableName::Stem, "b".into()),
                (VariableName::WorktreeRoot, path!("/dir").into()),
                (VariableName::Row, "1".into()),
                (VariableName::Column, "1".into()),
                (VariableName::Language, "Rust".into()),
            ]),
            project_env: HashMap::default(),
        }
    );

    editor2.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |selections| {
            selections.select_ranges([MultiBufferOffset(14)..MultiBufferOffset(18)])
        })
    });

    assert_eq!(
        workspace
            .update_in(cx, |workspace, window, cx| {
                task_contexts(workspace, window, cx)
            })
            .await
            .active_context()
            .expect("Should have an active context"),
        &TaskContext {
            cwd: Some(path!("/dir").into()),
            task_variables: TaskVariables::from_iter([
                (VariableName::File, path!("/dir/rust/b.rs").into()),
                (VariableName::Filename, "b.rs".into()),
                (VariableName::RelativeFile, path!("rust/b.rs").into()),
                (VariableName::RelativeDir, "rust".into()),
                (VariableName::Dirname, path!("/dir/rust").into()),
                (VariableName::Stem, "b".into()),
                (VariableName::WorktreeRoot, path!("/dir").into()),
                (VariableName::Row, "1".into()),
                (VariableName::Column, "15".into()),
                (VariableName::SelectedText, "is_i".into()),
                (VariableName::Symbol, "this_is_a_rust_file".into()),
                (VariableName::Language, "Rust".into()),
            ]),
            project_env: HashMap::default(),
        }
    );

    assert_eq!(
        workspace
            .update_in(cx, |workspace, window, cx| {
                workspace.activate_item(&editor1, true, true, window, cx);
                task_contexts(workspace, window, cx)
            })
            .await
            .active_context()
            .expect("Should have an active context"),
        &TaskContext {
            cwd: Some(path!("/dir").into()),
            task_variables: TaskVariables::from_iter([
                (VariableName::File, path!("/dir/a.ts").into()),
                (VariableName::Filename, "a.ts".into()),
                (VariableName::RelativeFile, "a.ts".into()),
                (VariableName::RelativeDir, ".".into()),
                (VariableName::Dirname, path!("/dir").into()),
                (VariableName::Stem, "a".into()),
                (VariableName::WorktreeRoot, path!("/dir").into()),
                (VariableName::Row, "1".into()),
                (VariableName::Column, "1".into()),
                (VariableName::Symbol, "this_is_a_test".into()),
                (VariableName::Language, "TypeScript".into()),
            ]),
            project_env: HashMap::default(),
        }
    );
}

pub(crate) fn init_test(cx: &mut TestAppContext) -> Arc<AppState> {
    cx.update(|cx| {
        let state = AppState::test(cx);
        crate::init(cx);
        editor::init(cx);
        TaskStore::init(None);
        state
    })
}
