use super::*;
use pretty_assertions::assert_eq;

#[gpui::test]
async fn test_external_editorconfig_not_loaded_without_internal_config(
    cx: &mut gpui::TestAppContext,
) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/parent"),
        json!({
            ".editorconfig": "[*]\nindent_size = 99\n",
            "worktree": {
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

        // file.rs should have default tab_size = 4, NOT 99 from parent's external .editorconfig
        // because without an internal .editorconfig, external configs are not loaded
        assert_eq!(Some(settings.tab_size), NonZeroU32::new(4));
    });
}

#[gpui::test]
async fn test_external_editorconfig_modification_triggers_refresh(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/parent"),
        json!({
            ".editorconfig": "[*]\nindent_size = 4\n",
            "worktree": {
                ".editorconfig": "[*]\n",
                "file.rs": "fn main() {}",
            }
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/parent/worktree").as_ref()], cx).await;

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

        // Test initial settings: tab_size = 4 from parent's external .editorconfig
        assert_eq!(Some(settings.tab_size), NonZeroU32::new(4));
    });

    fs.atomic_write(
        PathBuf::from(path!("/parent/.editorconfig")),
        "[*]\nindent_size = 8\n".to_owned(),
    )
    .await
    .unwrap();

    cx.executor().run_until_parked();

    let buffer = project
        .update(cx, |project, cx| {
            project.open_buffer((worktree.read(cx).id(), rel_path("file.rs")), cx)
        })
        .await
        .unwrap();

    cx.update(|cx| {
        let settings = LanguageSettings::for_buffer(&buffer.read(cx), cx);

        // Test settings updated: tab_size = 8
        assert_eq!(Some(settings.tab_size), NonZeroU32::new(8));
    });
}

#[gpui::test]
async fn test_adding_worktree_discovers_external_editorconfigs(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/parent"),
        json!({
            ".editorconfig": "root = true\n[*]\nindent_size = 7\n",
            "existing_worktree": {
                ".editorconfig": "[*]\n",
                "file.rs": "fn a() {}",
            },
            "new_worktree": {
                ".editorconfig": "[*]\n",
                "file.rs": "fn b() {}",
            }
        }),
    )
    .await;

    let project = Project::test(fs, [path!("/parent/existing_worktree").as_ref()], cx).await;

    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    language_registry.add(rust_lang());

    cx.executor().run_until_parked();

    let buffer = project
        .update(cx, |project, cx| {
            let id = project.worktrees(cx).next().unwrap().read(cx).id();
            project.open_buffer((id, rel_path("file.rs")), cx)
        })
        .await
        .unwrap();

    cx.update(|cx| {
        let settings = LanguageSettings::for_buffer(&buffer.read(cx), cx).into_owned();

        // Test existing worktree has tab_size = 7
        assert_eq!(Some(settings.tab_size), NonZeroU32::new(7));
    });

    let (new_worktree, _) = project
        .update(cx, |project, cx| {
            project.find_or_create_worktree(path!("/parent/new_worktree"), true, cx)
        })
        .await
        .unwrap();

    cx.executor().run_until_parked();

    let buffer = project
        .update(cx, |project, cx| {
            project.open_buffer((new_worktree.read(cx).id(), rel_path("file.rs")), cx)
        })
        .await
        .unwrap();

    cx.update(|cx| {
        let settings = LanguageSettings::for_buffer(&buffer.read(cx), cx);

        // Verify new worktree also has tab_size = 7 from shared parent editorconfig
        assert_eq!(Some(settings.tab_size), NonZeroU32::new(7));
    });
}

#[gpui::test]
async fn test_removing_worktree_cleans_up_external_editorconfig(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/parent"),
        json!({
            ".editorconfig": "[*]\nindent_size = 6\n",
            "worktree": {
                ".editorconfig": "[*]\n",
                "file.rs": "fn main() {}",
            }
        }),
    )
    .await;

    let project = Project::test(fs, [path!("/parent/worktree").as_ref()], cx).await;

    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    language_registry.add(rust_lang());

    let worktree = project.update(cx, |project, cx| project.worktrees(cx).next().unwrap());
    let worktree_id = worktree.read_with(cx, |tree, _| tree.id());

    cx.executor().run_until_parked();

    cx.update(|cx| {
        let store = cx.global::<SettingsStore>();
        let (worktree_ids, external_paths, watcher_paths) =
            store.editorconfig_store.read(cx).test_state();

        // Test external config is loaded
        assert!(worktree_ids.contains(&worktree_id));
        assert!(!external_paths.is_empty());
        assert!(!watcher_paths.is_empty());
    });

    project.update(cx, |project, cx| {
        project.remove_worktree(worktree_id, cx);
    });

    cx.executor().run_until_parked();

    cx.update(|cx| {
        let store = cx.global::<SettingsStore>();
        let (worktree_ids, external_paths, watcher_paths) =
            store.editorconfig_store.read(cx).test_state();

        // Test worktree state, external configs, and watchers all removed
        assert!(!worktree_ids.contains(&worktree_id));
        assert!(external_paths.is_empty());
        assert!(watcher_paths.is_empty());
    });
}

#[gpui::test]
async fn test_shared_external_editorconfig_cleanup_with_multiple_worktrees(
    cx: &mut gpui::TestAppContext,
) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/parent"),
        json!({
            ".editorconfig": "root = true\n[*]\nindent_size = 5\n",
            "worktree_a": {
                ".editorconfig": "[*]\n",
                "file.rs": "fn a() {}",
            },
            "worktree_b": {
                ".editorconfig": "[*]\n",
                "file.rs": "fn b() {}",
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

    let (worktree_a_id, worktree_b, worktree_b_id) = cx.update(|cx| {
        let worktrees: Vec<_> = project.read(cx).worktrees(cx).collect();
        assert_eq!(worktrees.len(), 2);

        let worktree_a = &worktrees[0];
        let worktree_b = &worktrees[1];
        let worktree_a_id = worktree_a.read(cx).id();
        let worktree_b_id = worktree_b.read(cx).id();
        (worktree_a_id, worktree_b.clone(), worktree_b_id)
    });

    cx.update(|cx| {
        let store = cx.global::<SettingsStore>();
        let (worktree_ids, external_paths, _) = store.editorconfig_store.read(cx).test_state();

        // Test both worktrees have settings and share external config
        assert!(worktree_ids.contains(&worktree_a_id));
        assert!(worktree_ids.contains(&worktree_b_id));
        assert_eq!(external_paths.len(), 1); // single shared external config
    });

    project.update(cx, |project, cx| {
        project.remove_worktree(worktree_a_id, cx);
    });

    cx.executor().run_until_parked();

    cx.update(|cx| {
        let store = cx.global::<SettingsStore>();
        let (worktree_ids, external_paths, watcher_paths) =
            store.editorconfig_store.read(cx).test_state();

        // Test worktree_a is gone but external config remains for worktree_b
        assert!(!worktree_ids.contains(&worktree_a_id));
        assert!(worktree_ids.contains(&worktree_b_id));
        // External config should still exist because worktree_b uses it
        assert_eq!(external_paths.len(), 1);
        assert_eq!(watcher_paths.len(), 1);
    });

    let buffer = project
        .update(cx, |project, cx| {
            project.open_buffer((worktree_b.read(cx).id(), rel_path("file.rs")), cx)
        })
        .await
        .unwrap();

    cx.update(|cx| {
        let settings = LanguageSettings::for_buffer(&buffer.read(cx), cx);

        // Test worktree_b still has correct settings
        assert_eq!(Some(settings.tab_size), NonZeroU32::new(5));
    });
}
