use super::*;
use pretty_assertions::assert_eq;

#[gpui::test]
async fn test_editorconfig_support(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let dir = TempTree::new(json!({
        ".editorconfig": r#"
        root = true
        [*.rs]
            indent_style = tab
            indent_size = 3
            end_of_line = lf
            insert_final_newline = true
            trim_trailing_whitespace = true
            max_line_length = 120
        [*.js]
            tab_width = 10
            max_line_length = off
        "#,
        ".mav": {
            "settings.json": r#"{
                "tab_size": 8,
                "hard_tabs": false,
                "ensure_final_newline_on_save": false,
                "remove_trailing_whitespace_on_save": false,
                "preferred_line_length": 64,
                "soft_wrap": "editor_width",
            }"#,
        },
        "a.rs": "fn a() {\n    A\n}",
        "b": {
            ".editorconfig": r#"
            [*.rs]
                indent_size = 2
                max_line_length = off,
            "#,
            "b.rs": "fn b() {\n    B\n}",
        },
        "c.js": "def c\n  C\nend",
        "d": {
            ".editorconfig": r#"
            [*.rs]
                indent_size = 1
            "#,
            "d.rs": "fn d() {\n    D\n}",
        },
        "e": {
            ".editorconfig": r#"
            [*.rs]
                indent_size = 5
                indent_style = space
                max_line_length =
            "#,
            "e.rs": "fn e() {\n    E\n}",
        },
        "README.json": "tabs are better\n",
    }));

    let path = dir.path();
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree_from_real_fs(path, path).await;
    let project = Project::test(fs, [path], cx).await;

    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    language_registry.add(js_lang());
    language_registry.add(json_lang());
    language_registry.add(rust_lang());

    let worktree = project.update(cx, |project, cx| project.worktrees(cx).next().unwrap());

    cx.executor().run_until_parked();

    let settings_for = async |path: &str, cx: &mut TestAppContext| -> LanguageSettings {
        let buffer = project
            .update(cx, |project, cx| {
                project.open_buffer((worktree.read(cx).id(), rel_path(path)), cx)
            })
            .await
            .unwrap();
        cx.update(|cx| LanguageSettings::for_buffer(&buffer.read(cx), cx).into_owned())
    };

    let settings_a = settings_for("a.rs", cx).await;
    let settings_b = settings_for("b/b.rs", cx).await;
    let settings_c = settings_for("c.js", cx).await;
    let settings_d = settings_for("d/d.rs", cx).await;
    let settings_e = settings_for("e/e.rs", cx).await;
    let settings_readme = settings_for("README.json", cx).await;
    // .editorconfig overrides .mav/settings
    assert_eq!(Some(settings_a.tab_size), NonZeroU32::new(3));
    assert_eq!(settings_a.hard_tabs, true);
    assert_eq!(settings_a.ensure_final_newline_on_save, true);
    assert_eq!(settings_a.remove_trailing_whitespace_on_save, true);
    assert_eq!(settings_a.line_ending, LineEndingSetting::EnforceLf);
    assert_eq!(settings_a.preferred_line_length, 120);

    // .editorconfig in b/ overrides .editorconfig in root
    assert_eq!(Some(settings_b.tab_size), NonZeroU32::new(2));

    // .editorconfig in subdirectory overrides .editorconfig in root
    assert_eq!(Some(settings_d.tab_size), NonZeroU32::new(1));

    // Non-empty values in e/ are parsed and applied as usual.
    assert_eq!(Some(settings_e.tab_size), NonZeroU32::new(5));
    assert_eq!(settings_e.hard_tabs, false);
    // An empty value opts out of the inherited `max_line_length = 120`,
    // falling back to .mav/settings.json instead of rejecting the whole file.
    assert_eq!(settings_e.preferred_line_length, 64);

    // "indent_size" is not set, so "tab_width" is used
    assert_eq!(Some(settings_c.tab_size), NonZeroU32::new(10));

    // When max_line_length is "off", default to .mav/settings.json
    assert_eq!(settings_b.preferred_line_length, 64);
    assert_eq!(settings_c.preferred_line_length, 64);

    // README.md should not be affected by .editorconfig's globe "*.rs"
    assert_eq!(Some(settings_readme.tab_size), NonZeroU32::new(8));
}

#[gpui::test]
async fn test_external_editorconfig_support(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/grandparent"),
        json!({
            ".editorconfig": "[*]\nindent_size = 4\n",
            "parent": {
                ".editorconfig": "[*.rs]\nindent_size = 2\n",
                "worktree": {
                    ".editorconfig": "[*.md]\nindent_size = 3\n",
                    "main.rs": "fn main() {}",
                    "README.md": "# README",
                    "other.txt": "other content",
                }
            }
        }),
    )
    .await;

    let project = Project::test(fs, [path!("/grandparent/parent/worktree").as_ref()], cx).await;

    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    language_registry.add(rust_lang());
    language_registry.add(markdown_lang());

    let worktree = project.update(cx, |project, cx| project.worktrees(cx).next().unwrap());

    cx.executor().run_until_parked();
    let settings_for = async |path: &str, cx: &mut TestAppContext| -> LanguageSettings {
        let buffer = project
            .update(cx, |project, cx| {
                project.open_buffer((worktree.read(cx).id(), rel_path(path)), cx)
            })
            .await
            .unwrap();
        cx.update(|cx| LanguageSettings::for_buffer(&buffer.read(cx), cx).into_owned())
    };

    let settings_rs = settings_for("main.rs", cx).await;
    let settings_md = settings_for("README.md", cx).await;
    let settings_txt = settings_for("other.txt", cx).await;

    // main.rs gets indent_size = 2 from parent's external .editorconfig
    assert_eq!(Some(settings_rs.tab_size), NonZeroU32::new(2));

    // README.md gets indent_size = 3 from internal worktree .editorconfig
    assert_eq!(Some(settings_md.tab_size), NonZeroU32::new(3));

    // other.txt gets indent_size = 4 from grandparent's external .editorconfig
    assert_eq!(Some(settings_txt.tab_size), NonZeroU32::new(4));
}

#[gpui::test]
async fn test_internal_editorconfig_root_stops_traversal(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/worktree"),
        json!({
            ".editorconfig": "[*]\nindent_size = 99\n",
            "src": {
                ".editorconfig": "root = true\n[*]\nindent_size = 2\n",
                "file.rs": "fn main() {}",
            }
        }),
    )
    .await;

    let project = Project::test(fs, [path!("/worktree").as_ref()], cx).await;

    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    language_registry.add(rust_lang());

    let worktree = project.update(cx, |project, cx| project.worktrees(cx).next().unwrap());

    cx.executor().run_until_parked();

    let buffer = project
        .update(cx, |project, cx| {
            project.open_buffer((worktree.read(cx).id(), rel_path("src/file.rs")), cx)
        })
        .await
        .unwrap();
    cx.update(|cx| {
        let settings = LanguageSettings::for_buffer(buffer.read(cx), cx).into_owned();
        assert_eq!(Some(settings.tab_size), NonZeroU32::new(2));
    });
}

#[gpui::test]
async fn test_external_editorconfig_root_stops_traversal(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/parent"),
        json!({
            ".editorconfig": "[*]\nindent_size = 99\n",
            "worktree": {
                ".editorconfig": "root = true\n[*]\nindent_size = 2\n",
                "file.rs": "fn main() {}",
            }
        }),
    )
    .await;

    let project = Project::test(fs, [path!("/parent/worktree").as_ref()], cx).await;

    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    language_registry.add(rust_lang());

    let worktree = project.update(cx, |project, cx| project.worktrees(cx).next().unwrap());

    cx.executor().run_until_parked();

    let buffer = project
        .update(cx, |project, cx| {
            project.open_buffer((worktree.read(cx).id(), rel_path("file.rs")), cx)
        })
        .await
        .unwrap();

    cx.update(|cx| {
        let settings = LanguageSettings::for_buffer(&buffer.read(cx), cx);

        // file.rs gets indent_size = 2 from worktree's root config, NOT 99 from parent
        assert_eq!(Some(settings.tab_size), NonZeroU32::new(2));
    });
}

#[gpui::test]
async fn test_external_editorconfig_root_in_parent_stops_traversal(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/grandparent"),
        json!({
            ".editorconfig": "[*]\nindent_size = 99\n",
            "parent": {
                ".editorconfig": "root = true\n[*]\nindent_size = 4\n",
                "worktree": {
                    "file.rs": "fn main() {}",
                }
            }
        }),
    )
    .await;

    let project = Project::test(fs, [path!("/grandparent/parent/worktree").as_ref()], cx).await;

    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    language_registry.add(rust_lang());

    let worktree = project.update(cx, |project, cx| project.worktrees(cx).next().unwrap());

    cx.executor().run_until_parked();

    let buffer = project
        .update(cx, |project, cx| {
            project.open_buffer((worktree.read(cx).id(), rel_path("file.rs")), cx)
        })
        .await
        .unwrap();

    cx.update(|cx| {
        let settings = LanguageSettings::for_buffer(&buffer.read(cx), cx);

        // file.rs gets indent_size = 4 from parent's root config, NOT 99 from grandparent
        assert_eq!(Some(settings.tab_size), NonZeroU32::new(4));
    });
}

#[gpui::test]
async fn test_external_editorconfig_shared_across_worktrees(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/parent"),
        json!({
            ".editorconfig": "root = true\n[*]\nindent_size = 5\n",
            "worktree_a": {
                "file.rs": "fn a() {}",
                ".editorconfig": "[*]\ninsert_final_newline = true\n",
            },
            "worktree_b": {
                "file.rs": "fn b() {}",
                ".editorconfig": "[*]\ninsert_final_newline = false\n",
            }
        }),
    )
    .await;

    let project = Project::test(
        fs,
        [
            path!("/parent/worktree_a").as_ref(),
            path!("/parent/worktree_b").as_ref(),
        ],
        cx,
    )
    .await;

    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    language_registry.add(rust_lang());

    cx.executor().run_until_parked();

    let worktrees: Vec<_> = cx.update(|cx| project.read(cx).worktrees(cx).collect());
    assert_eq!(worktrees.len(), 2);

    for worktree in worktrees {
        let buffer = project
            .update(cx, |project, cx| {
                project.open_buffer((worktree.read(cx).id(), rel_path("file.rs")), cx)
            })
            .await
            .unwrap();

        cx.update(|cx| {
            let settings = LanguageSettings::for_buffer(&buffer.read(cx), cx);

            // Both worktrees should get indent_size = 5 from shared parent .editorconfig
            assert_eq!(Some(settings.tab_size), NonZeroU32::new(5));
        });
    }
}
