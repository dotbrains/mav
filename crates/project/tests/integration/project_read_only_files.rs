use super::*;

#[gpui::test]
async fn test_read_only_files_setting(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    // Configure read_only_files setting
    cx.update(|cx| {
        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.project.worktree.read_only_files = Some(vec![
                    "**/generated/**".to_string(),
                    "**/*.gen.rs".to_string(),
                ]);
            });
        });
    });

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        path!("/root"),
        json!({
            "src": {
                "main.rs": "fn main() {}",
                "types.gen.rs": "// Generated file",
            },
            "generated": {
                "schema.rs": "// Auto-generated schema",
            }
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;

    // Open a regular file - should be read-write
    let regular_buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer(path!("/root/src/main.rs"), cx)
        })
        .await
        .unwrap();

    regular_buffer.read_with(cx, |buffer, _| {
        assert!(!buffer.read_only(), "Regular file should not be read-only");
    });

    // Open a file matching *.gen.rs pattern - should be read-only
    let gen_buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer(path!("/root/src/types.gen.rs"), cx)
        })
        .await
        .unwrap();

    gen_buffer.read_with(cx, |buffer, _| {
        assert!(
            buffer.read_only(),
            "File matching *.gen.rs pattern should be read-only"
        );
    });

    // Open a file in generated directory - should be read-only
    let generated_buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer(path!("/root/generated/schema.rs"), cx)
        })
        .await
        .unwrap();

    generated_buffer.read_with(cx, |buffer, _| {
        assert!(
            buffer.read_only(),
            "File in generated directory should be read-only"
        );
    });
}

#[gpui::test]
async fn test_read_only_files_empty_setting(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    // Explicitly set read_only_files to empty (default behavior)
    cx.update(|cx| {
        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.project.worktree.read_only_files = Some(vec![]);
            });
        });
    });

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        path!("/root"),
        json!({
            "src": {
                "main.rs": "fn main() {}",
            },
            "generated": {
                "schema.rs": "// Auto-generated schema",
            }
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;

    // All files should be read-write when read_only_files is empty
    let main_buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer(path!("/root/src/main.rs"), cx)
        })
        .await
        .unwrap();

    main_buffer.read_with(cx, |buffer, _| {
        assert!(
            !buffer.read_only(),
            "Files should not be read-only when read_only_files is empty"
        );
    });

    let generated_buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer(path!("/root/generated/schema.rs"), cx)
        })
        .await
        .unwrap();

    generated_buffer.read_with(cx, |buffer, _| {
        assert!(
            !buffer.read_only(),
            "Generated files should not be read-only when read_only_files is empty"
        );
    });
}

#[gpui::test]
async fn test_read_only_files_with_lock_files(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    // Configure to make lock files read-only
    cx.update(|cx| {
        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.project.worktree.read_only_files = Some(vec![
                    "**/*.lock".to_string(),
                    "**/package-lock.json".to_string(),
                ]);
            });
        });
    });

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        path!("/root"),
        json!({
            "Cargo.lock": "# Lock file",
            "Cargo.toml": "[package]",
            "package-lock.json": "{}",
            "package.json": "{}",
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;

    // Cargo.lock should be read-only
    let cargo_lock = project
        .update(cx, |project, cx| {
            project.open_local_buffer(path!("/root/Cargo.lock"), cx)
        })
        .await
        .unwrap();

    cargo_lock.read_with(cx, |buffer, _| {
        assert!(buffer.read_only(), "Cargo.lock should be read-only");
    });

    // Cargo.toml should be read-write
    let cargo_toml = project
        .update(cx, |project, cx| {
            project.open_local_buffer(path!("/root/Cargo.toml"), cx)
        })
        .await
        .unwrap();

    cargo_toml.read_with(cx, |buffer, _| {
        assert!(!buffer.read_only(), "Cargo.toml should not be read-only");
    });

    // package-lock.json should be read-only
    let package_lock = project
        .update(cx, |project, cx| {
            project.open_local_buffer(path!("/root/package-lock.json"), cx)
        })
        .await
        .unwrap();

    package_lock.read_with(cx, |buffer, _| {
        assert!(buffer.read_only(), "package-lock.json should be read-only");
    });

    // package.json should be read-write
    let package_json = project
        .update(cx, |project, cx| {
            project.open_local_buffer(path!("/root/package.json"), cx)
        })
        .await
        .unwrap();

    package_json.read_with(cx, |buffer, _| {
        assert!(!buffer.read_only(), "package.json should not be read-only");
    });
}
