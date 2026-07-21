#[cfg(test)]
mod project_settings_update_tests {
    use super::settings_file_updates::*;
    use super::*;
    use fs::{FakeFs, Fs as _};
    use gpui::TestAppContext;
    use project::Project;
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct TestSetup {
        fs: Arc<FakeFs>,
        project: Entity<Project>,
        worktree_id: WorktreeId,
        worktree: WeakEntity<Worktree>,
        rel_path: Arc<RelPath>,
        project_path: ProjectPath,
    }

    async fn init_test(cx: &mut TestAppContext, initial_settings: Option<&str>) -> TestSetup {
        cx.update(|cx| {
            let store = settings::SettingsStore::test(cx);
            cx.set_global(store);
            theme_settings::init(theme::LoadThemes::JustBase, cx);
            editor::init(cx);
            menu::init();
            let queue = ProjectSettingsUpdateQueue::new(cx);
            cx.set_global(queue);
        });

        let fs = FakeFs::new(cx.executor());
        let tree = if let Some(settings_content) = initial_settings {
            json!({
                ".mav": {
                    "settings.json": settings_content
                },
                "src": { "main.rs": "" }
            })
        } else {
            json!({ "src": { "main.rs": "" } })
        };
        fs.insert_tree("/project", tree).await;

        let project = Project::test(fs.clone(), ["/project".as_ref()], cx).await;

        let (worktree_id, worktree) = project.read_with(cx, |project, cx| {
            let worktree = project.worktrees(cx).next().unwrap();
            (worktree.read(cx).id(), worktree.downgrade())
        });

        let rel_path: Arc<RelPath> = RelPath::unix(".mav/settings.json")
            .expect("valid path")
            .into_arc();
        let project_path = ProjectPath {
            worktree_id,
            path: rel_path.clone(),
        };

        TestSetup {
            fs,
            project,
            worktree_id,
            worktree,
            rel_path,
            project_path,
        }
    }

    #[gpui::test]
    async fn test_creates_settings_file_if_missing(cx: &mut TestAppContext) {
        let setup = init_test(cx, None).await;

        let entry = ProjectSettingsUpdateEntry {
            worktree_id: setup.worktree_id,
            rel_path: setup.rel_path.clone(),
            settings_window: WeakEntity::new_invalid(),
            project: setup.project.downgrade(),
            worktree: setup.worktree,
            update: Box::new(|content, _cx| {
                content.project.all_languages.defaults.tab_size = Some(NonZeroU32::new(4).unwrap());
            }),
        };

        cx.update(|cx| ProjectSettingsUpdateQueue::enqueue(cx, entry));
        cx.executor().run_until_parked();

        let buffer_store = setup
            .project
            .read_with(cx, |project, _| project.buffer_store().clone());
        let buffer = buffer_store
            .update(cx, |store, cx| store.open_buffer(setup.project_path, cx))
            .await
            .expect("buffer should exist");

        let text = buffer.read_with(cx, |buffer, _| buffer.text());
        assert!(
            text.contains("\"tab_size\": 4"),
            "Expected tab_size setting in: {}",
            text
        );
    }

    #[gpui::test]
    async fn test_updates_existing_settings_file(cx: &mut TestAppContext) {
        let setup = init_test(cx, Some(r#"{ "tab_size": 2 }"#)).await;

        let entry = ProjectSettingsUpdateEntry {
            worktree_id: setup.worktree_id,
            rel_path: setup.rel_path.clone(),
            settings_window: WeakEntity::new_invalid(),
            project: setup.project.downgrade(),
            worktree: setup.worktree,
            update: Box::new(|content, _cx| {
                content.project.all_languages.defaults.tab_size = Some(NonZeroU32::new(8).unwrap());
            }),
        };

        cx.update(|cx| ProjectSettingsUpdateQueue::enqueue(cx, entry));
        cx.executor().run_until_parked();

        let buffer_store = setup
            .project
            .read_with(cx, |project, _| project.buffer_store().clone());
        let buffer = buffer_store
            .update(cx, |store, cx| store.open_buffer(setup.project_path, cx))
            .await
            .expect("buffer should exist");

        let text = buffer.read_with(cx, |buffer, _| buffer.text());
        assert!(
            text.contains("\"tab_size\": 8"),
            "Expected updated tab_size in: {}",
            text
        );
    }

    #[gpui::test]
    async fn test_updates_are_serialized(cx: &mut TestAppContext) {
        let setup = init_test(cx, Some("{}")).await;

        let update_order = Arc::new(std::sync::Mutex::new(Vec::new()));

        for i in 1..=3 {
            let update_order = update_order.clone();
            let entry = ProjectSettingsUpdateEntry {
                worktree_id: setup.worktree_id,
                rel_path: setup.rel_path.clone(),
                settings_window: WeakEntity::new_invalid(),
                project: setup.project.downgrade(),
                worktree: setup.worktree.clone(),
                update: Box::new(move |content, _cx| {
                    update_order.lock().unwrap().push(i);
                    content.project.all_languages.defaults.tab_size =
                        Some(NonZeroU32::new(i).unwrap());
                }),
            };
            cx.update(|cx| ProjectSettingsUpdateQueue::enqueue(cx, entry));
        }

        cx.executor().run_until_parked();

        let order = update_order.lock().unwrap().clone();
        assert_eq!(order, vec![1, 2, 3], "Updates should be processed in order");

        let buffer_store = setup
            .project
            .read_with(cx, |project, _| project.buffer_store().clone());
        let buffer = buffer_store
            .update(cx, |store, cx| store.open_buffer(setup.project_path, cx))
            .await
            .expect("buffer should exist");

        let text = buffer.read_with(cx, |buffer, _| buffer.text());
        assert!(
            text.contains("\"tab_size\": 3"),
            "Final tab_size should be 3: {}",
            text
        );
    }

    #[gpui::test]
    async fn test_queue_continues_after_failure(cx: &mut TestAppContext) {
        let setup = init_test(cx, Some("{}")).await;

        let successful_updates = Arc::new(AtomicUsize::new(0));

        {
            let successful_updates = successful_updates.clone();
            let entry = ProjectSettingsUpdateEntry {
                worktree_id: setup.worktree_id,
                rel_path: setup.rel_path.clone(),
                settings_window: WeakEntity::new_invalid(),
                project: setup.project.downgrade(),
                worktree: setup.worktree.clone(),
                update: Box::new(move |content, _cx| {
                    successful_updates.fetch_add(1, Ordering::SeqCst);
                    content.project.all_languages.defaults.tab_size =
                        Some(NonZeroU32::new(2).unwrap());
                }),
            };
            cx.update(|cx| ProjectSettingsUpdateQueue::enqueue(cx, entry));
        }

        {
            let entry = ProjectSettingsUpdateEntry {
                worktree_id: setup.worktree_id,
                rel_path: setup.rel_path.clone(),
                settings_window: WeakEntity::new_invalid(),
                project: WeakEntity::new_invalid(),
                worktree: setup.worktree.clone(),
                update: Box::new(|content, _cx| {
                    content.project.all_languages.defaults.tab_size =
                        Some(NonZeroU32::new(99).unwrap());
                }),
            };
            cx.update(|cx| ProjectSettingsUpdateQueue::enqueue(cx, entry));
        }

        {
            let successful_updates = successful_updates.clone();
            let entry = ProjectSettingsUpdateEntry {
                worktree_id: setup.worktree_id,
                rel_path: setup.rel_path.clone(),
                settings_window: WeakEntity::new_invalid(),
                project: setup.project.downgrade(),
                worktree: setup.worktree.clone(),
                update: Box::new(move |content, _cx| {
                    successful_updates.fetch_add(1, Ordering::SeqCst);
                    content.project.all_languages.defaults.tab_size =
                        Some(NonZeroU32::new(4).unwrap());
                }),
            };
            cx.update(|cx| ProjectSettingsUpdateQueue::enqueue(cx, entry));
        }

        cx.executor().run_until_parked();

        assert_eq!(
            successful_updates.load(Ordering::SeqCst),
            2,
            "Two updates should have succeeded despite middle failure"
        );

        let buffer_store = setup
            .project
            .read_with(cx, |project, _| project.buffer_store().clone());
        let buffer = buffer_store
            .update(cx, |store, cx| store.open_buffer(setup.project_path, cx))
            .await
            .expect("buffer should exist");

        let text = buffer.read_with(cx, |buffer, _| buffer.text());
        assert!(
            text.contains("\"tab_size\": 4"),
            "Final tab_size should be 4 (third update): {}",
            text
        );
    }

    #[gpui::test]
    async fn test_handles_dropped_worktree(cx: &mut TestAppContext) {
        let setup = init_test(cx, Some("{}")).await;

        let entry = ProjectSettingsUpdateEntry {
            worktree_id: setup.worktree_id,
            rel_path: setup.rel_path.clone(),
            settings_window: WeakEntity::new_invalid(),
            project: setup.project.downgrade(),
            worktree: WeakEntity::new_invalid(),
            update: Box::new(|content, _cx| {
                content.project.all_languages.defaults.tab_size =
                    Some(NonZeroU32::new(99).unwrap());
            }),
        };

        cx.update(|cx| ProjectSettingsUpdateQueue::enqueue(cx, entry));
        cx.executor().run_until_parked();

        let file_content = setup
            .fs
            .load("/project/.mav/settings.json".as_ref())
            .await
            .unwrap();
        assert_eq!(
            file_content, "{}",
            "File should be unchanged when worktree is dropped"
        );
    }

    #[gpui::test]
    async fn test_reloads_conflicted_buffer(cx: &mut TestAppContext) {
        let setup = init_test(cx, Some(r#"{ "tab_size": 2 }"#)).await;

        let buffer_store = setup
            .project
            .read_with(cx, |project, _| project.buffer_store().clone());
        let buffer = buffer_store
            .update(cx, |store, cx| {
                store.open_buffer(setup.project_path.clone(), cx)
            })
            .await
            .expect("buffer should exist");

        buffer.update(cx, |buffer, cx| {
            buffer.edit([(0..0, "// comment\n")], None, cx);
        });

        let has_unsaved_edits = buffer.read_with(cx, |buffer, _| buffer.has_unsaved_edits());
        assert!(has_unsaved_edits, "Buffer should have unsaved edits");

        setup
            .fs
            .save(
                "/project/.mav/settings.json".as_ref(),
                &r#"{ "tab_size": 99 }"#.into(),
                Default::default(),
            )
            .await
            .expect("save should succeed");

        cx.executor().run_until_parked();

        let has_conflict = buffer.read_with(cx, |buffer, _| buffer.has_conflict());
        assert!(
            has_conflict,
            "Buffer should have conflict after external modification"
        );

        let (settings_window, _) = cx.add_window_view(|window, cx| {
            let mut sw = SettingsWindow::test(window, cx);
            sw.project_setting_file_buffers
                .insert(setup.project_path.clone(), buffer.clone());
            sw
        });

        let entry = ProjectSettingsUpdateEntry {
            worktree_id: setup.worktree_id,
            rel_path: setup.rel_path.clone(),
            settings_window: settings_window.downgrade(),
            project: setup.project.downgrade(),
            worktree: setup.worktree.clone(),
            update: Box::new(|content, _cx| {
                content.project.all_languages.defaults.tab_size = Some(NonZeroU32::new(4).unwrap());
            }),
        };

        cx.update(|cx| ProjectSettingsUpdateQueue::enqueue(cx, entry));
        cx.executor().run_until_parked();

        let text = buffer.read_with(cx, |buffer, _| buffer.text());
        assert!(
            text.contains("\"tab_size\": 4"),
            "Buffer should have the new tab_size after reload and update: {}",
            text
        );
        assert!(
            !text.contains("// comment"),
            "Buffer should not contain the unsaved edit after reload: {}",
            text
        );
        assert!(
            !text.contains("99"),
            "Buffer should not contain the external modification value: {}",
            text
        );
    }
}
