use super::*;
use pretty_assertions::assert_eq;

#[gpui::test]
async fn test_buffer_line_endings(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/dir"),
        json!({
            "file1": "a\nb\nc\n",
            "file2": "one\r\ntwo\r\nthree\r\n",
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let buffer1 = project
        .update(cx, |p, cx| p.open_local_buffer(path!("/dir/file1"), cx))
        .await
        .unwrap();
    let buffer2 = project
        .update(cx, |p, cx| p.open_local_buffer(path!("/dir/file2"), cx))
        .await
        .unwrap();

    buffer1.update(cx, |buffer, _| {
        assert_eq!(buffer.text(), "a\nb\nc\n");
        assert_eq!(buffer.line_ending(), LineEnding::Unix);
    });
    buffer2.update(cx, |buffer, _| {
        assert_eq!(buffer.text(), "one\ntwo\nthree\n");
        assert_eq!(buffer.line_ending(), LineEnding::Windows);
    });

    // Change a file's line endings on disk from unix to windows. The buffer's
    // state updates correctly.
    fs.save(
        path!("/dir/file1").as_ref(),
        &"aaa\nb\nc\n".into(),
        LineEnding::Windows,
    )
    .await
    .unwrap();
    cx.executor().run_until_parked();
    buffer1.update(cx, |buffer, _| {
        assert_eq!(buffer.text(), "aaa\nb\nc\n");
        assert_eq!(buffer.line_ending(), LineEnding::Windows);
    });

    // Save a file with windows line endings. The file is written correctly.
    buffer2.update(cx, |buffer, cx| {
        buffer.set_text("one\ntwo\nthree\nfour\n", cx);
    });
    project
        .update(cx, |project, cx| project.save_buffer(buffer2, cx))
        .await
        .unwrap();
    assert_eq!(
        fs.load(path!("/dir/file2").as_ref()).await.unwrap(),
        "one\r\ntwo\r\nthree\r\nfour\r\n",
    );
}

#[gpui::test]
async fn test_line_ending_user_settings_on_format(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let cases = [
        (
            "default",
            None,
            [
                ("crlf_file.rs", LineEnding::Windows),
                ("lf_file.rs", LineEnding::Unix),
                ("no_newline.rs", LineEnding::default()),
            ],
        ),
        (
            "detect",
            Some(LineEndingSetting::Detect),
            [
                ("crlf_file.rs", LineEnding::Windows),
                ("lf_file.rs", LineEnding::Unix),
                ("no_newline.rs", LineEnding::default()),
            ],
        ),
        (
            "prefer_lf",
            Some(LineEndingSetting::PreferLf),
            [
                ("crlf_file.rs", LineEnding::Windows),
                ("lf_file.rs", LineEnding::Unix),
                ("no_newline.rs", LineEnding::Unix),
            ],
        ),
        (
            "prefer_crlf",
            Some(LineEndingSetting::PreferCrlf),
            [
                ("crlf_file.rs", LineEnding::Windows),
                ("lf_file.rs", LineEnding::Unix),
                ("no_newline.rs", LineEnding::Windows),
            ],
        ),
        (
            "enforce_lf",
            Some(LineEndingSetting::EnforceLf),
            [
                ("crlf_file.rs", LineEnding::Unix),
                ("lf_file.rs", LineEnding::Unix),
                ("no_newline.rs", LineEnding::Unix),
            ],
        ),
        (
            "enforce_crlf",
            Some(LineEndingSetting::EnforceCrlf),
            [
                ("crlf_file.rs", LineEnding::Windows),
                ("lf_file.rs", LineEnding::Windows),
                ("no_newline.rs", LineEnding::Windows),
            ],
        ),
    ];

    for (case_name, line_ending_setting, expected_line_endings) in cases {
        let fs = FakeFs::new(cx.executor());
        fs.insert_tree(
            path!("/dir"),
            json!({
                "crlf_file.rs": "one\r\ntwo\r\nthree\r\n",
                "lf_file.rs": "one\ntwo\nthree\n",
                "no_newline.rs": "single line",
            }),
        )
        .await;

        let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
        let language_registry = project.read_with(cx, |project, _| project.languages().clone());
        language_registry.add(rust_lang());
        let worktree_id = project.update(cx, |project, cx| {
            project.worktrees(cx).next().unwrap().read(cx).id()
        });

        cx.update(|cx| {
            SettingsStore::update_global(cx, |store, cx| {
                store.update_user_settings(cx, |settings| {
                    settings.project.all_languages.defaults.line_ending = line_ending_setting;
                });
            });
        });
        cx.executor().run_until_parked();

        assert_line_endings_after_format(
            cx,
            &project,
            worktree_id,
            case_name,
            &expected_line_endings,
        )
        .await;
    }
}

#[gpui::test]
async fn test_line_ending_editorconfig_on_format_and_save(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let cases = [
        (
            "editorconfig lf",
            "lf",
            "crlf_file.rs",
            LineEnding::Windows,
            [
                ("crlf_file.rs", LineEnding::Unix),
                ("lf_file.rs", LineEnding::Unix),
                ("no_newline.rs", LineEnding::Unix),
            ],
            "one\ntwo\nthree\n",
        ),
        (
            "editorconfig crlf",
            "crlf",
            "lf_file.rs",
            LineEnding::Unix,
            [
                ("crlf_file.rs", LineEnding::Windows),
                ("lf_file.rs", LineEnding::Windows),
                ("no_newline.rs", LineEnding::Windows),
            ],
            "one\r\ntwo\r\nthree\r\n",
        ),
    ];

    for (
        case_name,
        editorconfig_end_of_line,
        buffer_path,
        initial_line_ending,
        expected_line_endings,
        expected_saved_contents,
    ) in cases
    {
        let file_system = FakeFs::new(cx.executor());
        file_system
            .insert_tree(
                path!("/dir"),
                json!({
                    ".editorconfig": format!("root = true\n[*.rs]\nend_of_line = {editorconfig_end_of_line}\n"),
                    "crlf_file.rs": "one\r\ntwo\r\nthree\r\n",
                    "lf_file.rs": "one\ntwo\nthree\n",
                    "no_newline.rs": "single line",
                }),
            )
            .await;

        let project = Project::test(file_system.clone(), [path!("/dir").as_ref()], cx).await;
        let language_registry = project.read_with(cx, |project, _| project.languages().clone());
        language_registry.add(rust_lang());
        cx.executor().run_until_parked();
        let worktree_id = project.update(cx, |project, cx| {
            project.worktrees(cx).next().unwrap().read(cx).id()
        });

        let buffer = project
            .update(cx, |project, cx| {
                project.open_buffer((worktree_id, rel_path(buffer_path)), cx)
            })
            .await
            .unwrap();
        buffer.update(cx, |buffer, _| {
            assert_eq!(buffer.line_ending(), initial_line_ending);
        });

        assert_line_endings_after_format(
            cx,
            &project,
            worktree_id,
            case_name,
            &expected_line_endings,
        )
        .await;

        project
            .update(cx, |project, cx| project.save_buffer(buffer, cx))
            .await
            .unwrap();
        let saved_path = PathBuf::from(path!("/dir")).join(buffer_path);
        assert_eq!(
            file_system.load(&saved_path).await.unwrap(),
            expected_saved_contents,
        );
    }
}

#[gpui::test]
async fn test_line_ending_initialization_for_new_buffers(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let cases = [
        (Some(LineEndingSetting::Detect), LineEnding::default()),
        (Some(LineEndingSetting::PreferLf), LineEnding::Unix),
        (Some(LineEndingSetting::PreferCrlf), LineEnding::Windows),
        (Some(LineEndingSetting::EnforceLf), LineEnding::Unix),
        (Some(LineEndingSetting::EnforceCrlf), LineEnding::Windows),
    ];

    for (line_ending_setting, expected_line_ending) in cases {
        let fs = FakeFs::new(cx.executor());
        fs.insert_tree(path!("/dir"), json!({})).await;

        let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
        cx.update(|cx| {
            SettingsStore::update_global(cx, |store, cx| {
                store.update_user_settings(cx, |settings| {
                    settings.project.all_languages.defaults.line_ending = line_ending_setting;
                });
            });
        });
        cx.executor().run_until_parked();

        let created_buffer = project
            .update(cx, |project, cx| project.create_buffer(None, false, cx))
            .unwrap()
            .await;
        created_buffer.update(cx, |buffer, _| {
            assert_eq!(buffer.line_ending(), expected_line_ending);
        });

        let local_buffer = project.update(cx, |project, cx| {
            project.create_local_buffer("single line", None, false, cx)
        });
        local_buffer.update(cx, |buffer, _| {
            assert_eq!(buffer.line_ending(), expected_line_ending);
        });

        let opened_missing_buffer = project
            .update(cx, |project, cx| {
                project.open_local_buffer(path!("/dir/new_file.rs"), cx)
            })
            .await
            .unwrap();
        opened_missing_buffer.update(cx, |buffer, _| {
            assert_eq!(buffer.line_ending(), expected_line_ending);
        });
    }
}

async fn assert_line_endings_after_format(
    cx: &mut gpui::TestAppContext,
    project: &Entity<Project>,
    worktree_id: WorktreeId,
    case_name: &str,
    expected_line_endings: &[(&str, LineEnding)],
) {
    for (path, expected_line_ending) in expected_line_endings {
        let buffer = project
            .update(cx, |project, cx| {
                project.open_buffer((worktree_id, rel_path(path)), cx)
            })
            .await
            .unwrap();
        let mut buffers = HashSet::default();
        buffers.insert(buffer.clone());
        project
            .update(cx, |project, cx| {
                project.format(
                    buffers,
                    project::lsp_store::LspFormatTarget::Buffers,
                    false,
                    project::lsp_store::FormatTrigger::Save,
                    cx,
                )
            })
            .await
            .unwrap();
        buffer.update(cx, |buffer, _| {
            assert_eq!(
                buffer.line_ending(),
                *expected_line_ending,
                "unexpected line ending for {path} in {case_name}"
            );
        });
    }
}
