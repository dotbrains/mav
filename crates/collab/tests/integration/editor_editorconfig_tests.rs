use crate::TestServer;
use call::ActiveCall;
use editor::{
    Editor,
    test::editor_test_context::{AssertionContextManager, EditorTestContext},
};
use fs::Fs;
use gpui::{TestAppContext, VisualContext};
use indoc::indoc;
use language::{language_settings::LanguageSettings, rust_lang};
use pretty_assertions::assert_eq;
use serde_json::json;
use std::{num::NonZeroU32, path::PathBuf};
use util::{path, rel_path::rel_path};

#[gpui::test(iterations = 30)]
async fn test_collaborating_with_editorconfig(
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
) {
    let mut server = TestServer::start(cx_a.executor()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    server
        .create_room(&mut [(&client_a, cx_a), (&client_b, cx_b)])
        .await;
    let active_call_a = cx_a.read(ActiveCall::global);

    cx_b.update(editor::init);

    // Set up a fake language server.
    client_a.language_registry().add(rust_lang());
    client_a
        .fs()
        .insert_tree(
            path!("/a"),
            json!({
                "src": {
                    "main.rs": "mod other;\nfn main() { let foo = other::foo(); }",
                    "other_mod": {
                        "other.rs": "pub fn foo() -> usize {\n    4\n}",
                        ".editorconfig": "",
                    },
                },
                ".editorconfig": "[*]\ntab_width = 2\n",
            }),
        )
        .await;
    let (project_a, worktree_id) = client_a.build_local_project(path!("/a"), cx_a).await;
    let project_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a.clone(), cx))
        .await
        .unwrap();
    let main_buffer_a = project_a
        .update(cx_a, |p, cx| {
            p.open_buffer((worktree_id, rel_path("src/main.rs")), cx)
        })
        .await
        .unwrap();
    let other_buffer_a = project_a
        .update(cx_a, |p, cx| {
            p.open_buffer((worktree_id, rel_path("src/other_mod/other.rs")), cx)
        })
        .await
        .unwrap();
    let cx_a = cx_a.add_empty_window();
    let main_editor_a = cx_a.new_window_entity(|window, cx| {
        Editor::for_buffer(main_buffer_a, Some(project_a.clone()), window, cx)
    });
    let other_editor_a = cx_a.new_window_entity(|window, cx| {
        Editor::for_buffer(other_buffer_a, Some(project_a), window, cx)
    });
    let mut main_editor_cx_a = EditorTestContext {
        cx: cx_a.clone(),
        window: cx_a.window_handle(),
        editor: main_editor_a,
        assertion_cx: AssertionContextManager::new(),
    };
    let mut other_editor_cx_a = EditorTestContext {
        cx: cx_a.clone(),
        window: cx_a.window_handle(),
        editor: other_editor_a,
        assertion_cx: AssertionContextManager::new(),
    };

    // Join the project as client B.
    let project_b = client_b.join_remote_project(project_id, cx_b).await;
    let main_buffer_b = project_b
        .update(cx_b, |p, cx| {
            p.open_buffer((worktree_id, rel_path("src/main.rs")), cx)
        })
        .await
        .unwrap();
    let other_buffer_b = project_b
        .update(cx_b, |p, cx| {
            p.open_buffer((worktree_id, rel_path("src/other_mod/other.rs")), cx)
        })
        .await
        .unwrap();
    let cx_b = cx_b.add_empty_window();
    let main_editor_b = cx_b.new_window_entity(|window, cx| {
        Editor::for_buffer(main_buffer_b, Some(project_b.clone()), window, cx)
    });
    let other_editor_b = cx_b.new_window_entity(|window, cx| {
        Editor::for_buffer(other_buffer_b, Some(project_b.clone()), window, cx)
    });
    let mut main_editor_cx_b = EditorTestContext {
        cx: cx_b.clone(),
        window: cx_b.window_handle(),
        editor: main_editor_b,
        assertion_cx: AssertionContextManager::new(),
    };
    let mut other_editor_cx_b = EditorTestContext {
        cx: cx_b.clone(),
        window: cx_b.window_handle(),
        editor: other_editor_b,
        assertion_cx: AssertionContextManager::new(),
    };

    let initial_main = indoc! {"
ˇmod other;
fn main() { let foo = other::foo(); }"};
    let initial_other = indoc! {"
ˇpub fn foo() -> usize {
    4
}"};

    let first_tabbed_main = indoc! {"
  ˇmod other;
fn main() { let foo = other::foo(); }"};
    tab_undo_assert(
        &mut main_editor_cx_a,
        &mut main_editor_cx_b,
        initial_main,
        first_tabbed_main,
        true,
    );
    tab_undo_assert(
        &mut main_editor_cx_a,
        &mut main_editor_cx_b,
        initial_main,
        first_tabbed_main,
        false,
    );

    let first_tabbed_other = indoc! {"
  ˇpub fn foo() -> usize {
    4
}"};
    tab_undo_assert(
        &mut other_editor_cx_a,
        &mut other_editor_cx_b,
        initial_other,
        first_tabbed_other,
        true,
    );
    tab_undo_assert(
        &mut other_editor_cx_a,
        &mut other_editor_cx_b,
        initial_other,
        first_tabbed_other,
        false,
    );

    client_a
        .fs()
        .atomic_write(
            PathBuf::from(path!("/a/src/.editorconfig")),
            "[*]\ntab_width = 3\n".to_owned(),
        )
        .await
        .unwrap();
    cx_a.run_until_parked();
    cx_b.run_until_parked();

    let second_tabbed_main = indoc! {"
   ˇmod other;
fn main() { let foo = other::foo(); }"};
    tab_undo_assert(
        &mut main_editor_cx_a,
        &mut main_editor_cx_b,
        initial_main,
        second_tabbed_main,
        true,
    );
    tab_undo_assert(
        &mut main_editor_cx_a,
        &mut main_editor_cx_b,
        initial_main,
        second_tabbed_main,
        false,
    );

    let second_tabbed_other = indoc! {"
   ˇpub fn foo() -> usize {
    4
}"};
    tab_undo_assert(
        &mut other_editor_cx_a,
        &mut other_editor_cx_b,
        initial_other,
        second_tabbed_other,
        true,
    );
    tab_undo_assert(
        &mut other_editor_cx_a,
        &mut other_editor_cx_b,
        initial_other,
        second_tabbed_other,
        false,
    );

    let editorconfig_buffer_b = project_b
        .update(cx_b, |p, cx| {
            p.open_buffer((worktree_id, rel_path("src/other_mod/.editorconfig")), cx)
        })
        .await
        .unwrap();
    editorconfig_buffer_b.update(cx_b, |buffer, cx| {
        buffer.set_text("[*.rs]\ntab_width = 6\n", cx);
    });
    project_b
        .update(cx_b, |project, cx| {
            project.save_buffer(editorconfig_buffer_b.clone(), cx)
        })
        .await
        .unwrap();
    cx_a.run_until_parked();
    cx_b.run_until_parked();

    tab_undo_assert(
        &mut main_editor_cx_a,
        &mut main_editor_cx_b,
        initial_main,
        second_tabbed_main,
        true,
    );
    tab_undo_assert(
        &mut main_editor_cx_a,
        &mut main_editor_cx_b,
        initial_main,
        second_tabbed_main,
        false,
    );

    let third_tabbed_other = indoc! {"
      ˇpub fn foo() -> usize {
    4
}"};
    tab_undo_assert(
        &mut other_editor_cx_a,
        &mut other_editor_cx_b,
        initial_other,
        third_tabbed_other,
        true,
    );

    tab_undo_assert(
        &mut other_editor_cx_a,
        &mut other_editor_cx_b,
        initial_other,
        third_tabbed_other,
        false,
    );
}

#[gpui::test(iterations = 10)]
async fn test_collaborating_with_external_editorconfig(
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
) {
    let mut server = TestServer::start(cx_a.executor()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    server
        .create_room(&mut [(&client_a, cx_a), (&client_b, cx_b)])
        .await;
    let active_call_a = cx_a.read(ActiveCall::global);

    client_a.language_registry().add(rust_lang());
    client_b.language_registry().add(rust_lang());

    // Set up external .editorconfig in parent directory
    client_a
        .fs()
        .insert_tree(
            path!("/parent"),
            json!({
                ".editorconfig": "[*]\nindent_size = 5\n",
                "worktree": {
                    ".editorconfig": "[*]\n",
                    "src": {
                        "main.rs": "fn main() {}",
                    },
                },
            }),
        )
        .await;

    let (project_a, worktree_id) = client_a
        .build_local_project(path!("/parent/worktree"), cx_a)
        .await;
    let project_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a.clone(), cx))
        .await
        .unwrap();

    project_a.update(cx_a, |project, _| project.languages().add(rust_lang()));

    // Open buffer on client A
    let buffer_a = project_a
        .update(cx_a, |p, cx| {
            p.open_buffer((worktree_id, rel_path("src/main.rs")), cx)
        })
        .await
        .unwrap();

    cx_a.run_until_parked();

    // Verify client A sees external editorconfig settings
    cx_a.read(|cx| {
        let settings = LanguageSettings::for_buffer(&buffer_a.read(cx), cx);
        assert_eq!(Some(settings.tab_size), NonZeroU32::new(5));
    });

    // Client B joins the project
    let project_b = client_b.join_remote_project(project_id, cx_b).await;
    project_b.update(cx_b, |project, _| project.languages().add(rust_lang()));
    let buffer_b = project_b
        .update(cx_b, |p, cx| {
            p.open_buffer((worktree_id, rel_path("src/main.rs")), cx)
        })
        .await
        .unwrap();

    cx_b.run_until_parked();

    // Verify client B also sees external editorconfig settings
    cx_b.read(|cx| {
        let settings = LanguageSettings::for_buffer(&buffer_b.read(cx), cx);
        assert_eq!(Some(settings.tab_size), NonZeroU32::new(5));
    });

    // Client A modifies the external .editorconfig
    client_a
        .fs()
        .atomic_write(
            PathBuf::from(path!("/parent/.editorconfig")),
            "[*]\nindent_size = 9\n".to_owned(),
        )
        .await
        .unwrap();

    cx_a.run_until_parked();
    cx_b.run_until_parked();

    // Verify client A sees updated settings
    cx_a.read(|cx| {
        let settings = LanguageSettings::for_buffer(&buffer_a.read(cx), cx);
        assert_eq!(Some(settings.tab_size), NonZeroU32::new(9));
    });

    // Verify client B also sees updated settings
    cx_b.read(|cx| {
        let settings = LanguageSettings::for_buffer(&buffer_b.read(cx), cx);
        assert_eq!(Some(settings.tab_size), NonZeroU32::new(9));
    });
}

fn tab_undo_assert(
    cx_a: &mut EditorTestContext,
    cx_b: &mut EditorTestContext,
    expected_initial: &str,
    expected_tabbed: &str,
    a_tabs: bool,
) {
    cx_a.assert_editor_state(expected_initial);
    cx_b.assert_editor_state(expected_initial);

    if a_tabs {
        cx_a.update_editor(|editor, window, cx| {
            editor.tab(&editor::actions::Tab, window, cx);
        });
    } else {
        cx_b.update_editor(|editor, window, cx| {
            editor.tab(&editor::actions::Tab, window, cx);
        });
    }

    cx_a.run_until_parked();
    cx_b.run_until_parked();

    cx_a.assert_editor_state(expected_tabbed);
    cx_b.assert_editor_state(expected_tabbed);

    if a_tabs {
        cx_a.update_editor(|editor, window, cx| {
            editor.undo(&editor::actions::Undo, window, cx);
        });
    } else {
        cx_b.update_editor(|editor, window, cx| {
            editor.undo(&editor::actions::Undo, window, cx);
        });
    }
    cx_a.run_until_parked();
    cx_b.run_until_parked();
    cx_a.assert_editor_state(expected_initial);
    cx_b.assert_editor_state(expected_initial);
}
