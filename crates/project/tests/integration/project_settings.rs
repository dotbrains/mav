use super::*;
use pretty_assertions::assert_eq;

#[gpui::test]
async fn test_git_provider_project_setting(cx: &mut gpui::TestAppContext) {
    init_test(cx);
    cx.update(|cx| {
        GitHostingProviderRegistry::default_global(cx);
        git_hosting_providers::init(cx);
    });

    let fs = FakeFs::new(cx.executor());
    let str_path = path!("/dir");
    let path = Path::new(str_path);

    fs.insert_tree(
        path!("/dir"),
        json!({
            ".mav": {
                "settings.json": r#"{
                    "git_hosting_providers": [
                        {
                            "provider": "gitlab",
                            "base_url": "https://google.com",
                            "name": "foo"
                        }
                    ]
                }"#
            },
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let (_worktree, _) =
        project.read_with(cx, |project, cx| project.find_worktree(path, cx).unwrap());
    cx.executor().run_until_parked();

    cx.update(|cx| {
        let provider = GitHostingProviderRegistry::global(cx);
        assert!(
            provider
                .list_hosting_providers()
                .into_iter()
                .any(|provider| provider.name() == "foo")
        );
    });

    fs.atomic_write(
        Path::new(path!("/dir/.mav/settings.json")).to_owned(),
        "{}".into(),
    )
    .await
    .unwrap();

    cx.run_until_parked();

    cx.update(|cx| {
        let provider = GitHostingProviderRegistry::global(cx);
        assert!(
            !provider
                .list_hosting_providers()
                .into_iter()
                .any(|provider| provider.name() == "foo")
        );
    });
}

#[gpui::test]
async fn test_managing_project_specific_settings(cx: &mut gpui::TestAppContext) {
    init_test(cx);
    TaskStore::init(None);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/dir"),
        json!({
            ".mav": {
                "settings.json": r#"{ "tab_size": 8 }"#,
                "tasks.json": r#"[{
                    "label": "cargo check all",
                    "command": "cargo",
                    "args": ["check", "--all"]
                },]"#,
            },
            "a": {
                "a.rs": "fn a() {\n    A\n}"
            },
            "b": {
                ".mav": {
                    "settings.json": r#"{ "tab_size": 2 }"#,
                    "tasks.json": r#"[{
                        "label": "cargo check",
                        "command": "cargo",
                        "args": ["check"]
                    },]"#,
                },
                "b.rs": "fn b() {\n  B\n}"
            }
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let worktree = project.update(cx, |project, cx| project.worktrees(cx).next().unwrap());

    cx.executor().run_until_parked();
    let worktree_id = cx.update(|cx| {
        project.update(cx, |project, cx| {
            project.worktrees(cx).next().unwrap().read(cx).id()
        })
    });

    let mut task_contexts = TaskContexts::default();
    task_contexts.active_worktree_context = Some((worktree_id, TaskContext::default()));
    let task_contexts = Arc::new(task_contexts);

    let topmost_local_task_source_kind = TaskSourceKind::Worktree {
        id: worktree_id,
        directory_in_worktree: rel_path(".mav").into(),
        id_base: "local worktree tasks from directory \".mav\"".into(),
    };

    let buffer_a = project
        .update(cx, |project, cx| {
            project.open_buffer((worktree.read(cx).id(), rel_path("a/a.rs")), cx)
        })
        .await
        .unwrap();
    let buffer_b = project
        .update(cx, |project, cx| {
            project.open_buffer((worktree.read(cx).id(), rel_path("b/b.rs")), cx)
        })
        .await
        .unwrap();
    cx.update(|cx| {
        let settings_a = LanguageSettings::for_buffer(&buffer_a.read(cx), cx);
        let settings_b = LanguageSettings::for_buffer(&buffer_b.read(cx), cx);

        assert_eq!(settings_a.tab_size.get(), 8);
        assert_eq!(settings_b.tab_size.get(), 2);
    });

    let all_tasks = cx
        .update(|cx| get_all_tasks(&project, task_contexts.clone(), cx))
        .await
        .into_iter()
        .map(|(source_kind, task)| {
            let resolved = task.resolved;
            (
                source_kind,
                task.resolved_label,
                resolved.args,
                resolved.env,
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        all_tasks,
        vec![
            (
                TaskSourceKind::Worktree {
                    id: worktree_id,
                    directory_in_worktree: rel_path("b/.mav").into(),
                    id_base: "local worktree tasks from directory \"b/.mav\"".into()
                },
                "cargo check".to_string(),
                vec!["check".to_string()],
                HashMap::default(),
            ),
            (
                topmost_local_task_source_kind.clone(),
                "cargo check all".to_string(),
                vec!["check".to_string(), "--all".to_string()],
                HashMap::default(),
            ),
        ]
    );

    let (_, resolved_task) = cx
        .update(|cx| get_all_tasks(&project, task_contexts.clone(), cx))
        .await
        .into_iter()
        .find(|(source_kind, _)| source_kind == &topmost_local_task_source_kind)
        .expect("should have one global task");
    project.update(cx, |project, cx| {
        let task_inventory = project
            .task_store()
            .read(cx)
            .task_inventory()
            .cloned()
            .unwrap();
        task_inventory.update(cx, |inventory, _| {
            inventory.task_scheduled(topmost_local_task_source_kind.clone(), resolved_task);
            inventory
                .update_file_based_tasks(
                    TaskSettingsLocation::Global(tasks_file()),
                    Some(
                        &json!([{
                            "label": "cargo check unstable",
                            "command": "cargo",
                            "args": [
                                "check",
                                "--all",
                                "--all-targets"
                            ],
                            "env": {
                                "RUSTFLAGS": "-Zunstable-options"
                            }
                        }])
                        .to_string(),
                    ),
                )
                .unwrap();
        });
    });
    cx.run_until_parked();

    let all_tasks = cx
        .update(|cx| get_all_tasks(&project, task_contexts.clone(), cx))
        .await
        .into_iter()
        .map(|(source_kind, task)| {
            let resolved = task.resolved;
            (
                source_kind,
                task.resolved_label,
                resolved.args,
                resolved.env,
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        all_tasks,
        vec![
            (
                topmost_local_task_source_kind.clone(),
                "cargo check all".to_string(),
                vec!["check".to_string(), "--all".to_string()],
                HashMap::default(),
            ),
            (
                TaskSourceKind::Worktree {
                    id: worktree_id,
                    directory_in_worktree: rel_path("b/.mav").into(),
                    id_base: "local worktree tasks from directory \"b/.mav\"".into()
                },
                "cargo check".to_string(),
                vec!["check".to_string()],
                HashMap::default(),
            ),
            (
                TaskSourceKind::AbsPath {
                    abs_path: paths::tasks_file().clone(),
                    id_base: "global tasks.json".into(),
                },
                "cargo check unstable".to_string(),
                vec![
                    "check".to_string(),
                    "--all".to_string(),
                    "--all-targets".to_string(),
                ],
                HashMap::from_iter(Some((
                    "RUSTFLAGS".to_string(),
                    "-Zunstable-options".to_string()
                ))),
            ),
        ]
    );
}
