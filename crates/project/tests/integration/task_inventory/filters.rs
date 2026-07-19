use super::*;

#[gpui::test]
async fn test_inventory_static_task_filters(cx: &mut TestAppContext) {
    init_test(cx);
    let inventory = cx.update(|cx| Inventory::new(cx));
    let common_name = "common_task_name";
    let worktree_1 = WorktreeId::from_usize(1);
    let worktree_2 = WorktreeId::from_usize(2);

    cx.run_until_parked();
    let worktree_independent_tasks = vec![
        (
            TaskSourceKind::AbsPath {
                id_base: "global tasks.json".into(),
                abs_path: paths::tasks_file().clone(),
            },
            common_name.to_string(),
        ),
        (
            TaskSourceKind::AbsPath {
                id_base: "global tasks.json".into(),
                abs_path: paths::tasks_file().clone(),
            },
            "static_source_1".to_string(),
        ),
        (
            TaskSourceKind::AbsPath {
                id_base: "global tasks.json".into(),
                abs_path: paths::tasks_file().clone(),
            },
            "static_source_2".to_string(),
        ),
    ];
    let worktree_1_tasks = [
        (
            TaskSourceKind::Worktree {
                id: worktree_1,
                directory_in_worktree: rel_path(".mav").into(),
                id_base: "local worktree tasks from directory \".mav\"".into(),
            },
            common_name.to_string(),
        ),
        (
            TaskSourceKind::Worktree {
                id: worktree_1,
                directory_in_worktree: rel_path(".mav").into(),
                id_base: "local worktree tasks from directory \".mav\"".into(),
            },
            "worktree_1".to_string(),
        ),
    ];
    let worktree_2_tasks = [
        (
            TaskSourceKind::Worktree {
                id: worktree_2,
                directory_in_worktree: rel_path(".mav").into(),
                id_base: "local worktree tasks from directory \".mav\"".into(),
            },
            common_name.to_string(),
        ),
        (
            TaskSourceKind::Worktree {
                id: worktree_2,
                directory_in_worktree: rel_path(".mav").into(),
                id_base: "local worktree tasks from directory \".mav\"".into(),
            },
            "worktree_2".to_string(),
        ),
    ];

    inventory.update(cx, |inventory, _| {
        inventory
            .update_file_based_tasks(
                TaskSettingsLocation::Global(tasks_file()),
                Some(&mock_tasks_from_names(
                    worktree_independent_tasks
                        .iter()
                        .map(|(_, name)| name.as_str()),
                )),
            )
            .unwrap();
        inventory
            .update_file_based_tasks(
                TaskSettingsLocation::Worktree(SettingsLocation {
                    worktree_id: worktree_1,
                    path: rel_path(".mav"),
                }),
                Some(&mock_tasks_from_names(
                    worktree_1_tasks.iter().map(|(_, name)| name.as_str()),
                )),
            )
            .unwrap();
        inventory
            .update_file_based_tasks(
                TaskSettingsLocation::Worktree(SettingsLocation {
                    worktree_id: worktree_2,
                    path: rel_path(".mav"),
                }),
                Some(&mock_tasks_from_names(
                    worktree_2_tasks.iter().map(|(_, name)| name.as_str()),
                )),
            )
            .unwrap();
    });

    pretty_assertions::assert_eq!(
        list_tasks_sorted_by_last_used(&inventory, None, cx).await,
        worktree_independent_tasks,
        "Without a worktree, only worktree-independent tasks should be listed"
    );
    pretty_assertions::assert_eq!(
        list_tasks_sorted_by_last_used(&inventory, Some(worktree_1), cx).await,
        worktree_1_tasks
            .iter()
            .chain(worktree_independent_tasks.iter())
            .cloned()
            .sorted_by_key(|(kind, label)| (task_source_kind_preference(kind), label.clone()))
            .collect::<Vec<_>>(),
    );
    pretty_assertions::assert_eq!(
        list_tasks_sorted_by_last_used(&inventory, Some(worktree_2), cx).await,
        worktree_2_tasks
            .iter()
            .chain(worktree_independent_tasks.iter())
            .cloned()
            .sorted_by_key(|(kind, label)| (task_source_kind_preference(kind), label.clone()))
            .collect::<Vec<_>>(),
    );

    pretty_assertions::assert_eq!(
        list_tasks(&inventory, None, cx).await,
        worktree_independent_tasks,
        "Without a worktree, only worktree-independent tasks should be listed"
    );
    pretty_assertions::assert_eq!(
        list_tasks(&inventory, Some(worktree_1), cx).await,
        worktree_1_tasks
            .iter()
            .chain(worktree_independent_tasks.iter())
            .cloned()
            .collect::<Vec<_>>(),
    );
    pretty_assertions::assert_eq!(
        list_tasks(&inventory, Some(worktree_2), cx).await,
        worktree_2_tasks
            .iter()
            .chain(worktree_independent_tasks.iter())
            .cloned()
            .collect::<Vec<_>>(),
    );
}

#[gpui::test]
async fn test_mav_tasks_take_precedence_over_vscode(cx: &mut TestAppContext) {
    init_test(cx);
    let inventory = cx.update(|cx| Inventory::new(cx));
    let worktree_id = WorktreeId::from_usize(0);

    inventory.update(cx, |inventory, _| {
        inventory
            .update_file_based_tasks(
                TaskSettingsLocation::Worktree(SettingsLocation {
                    worktree_id,
                    path: rel_path(".vscode"),
                }),
                Some(&mock_tasks_from_names(["vscode_task"])),
            )
            .unwrap();
    });
    pretty_assertions::assert_eq!(
        task_template_names(&inventory, Some(worktree_id), cx).await,
        vec!["vscode_task"],
        "With only .vscode tasks, they should appear"
    );

    inventory.update(cx, |inventory, _| {
        inventory
            .update_file_based_tasks(
                TaskSettingsLocation::Worktree(SettingsLocation {
                    worktree_id,
                    path: rel_path(".mav"),
                }),
                Some(&mock_tasks_from_names(["mav_task"])),
            )
            .unwrap();
    });
    pretty_assertions::assert_eq!(
        task_template_names(&inventory, Some(worktree_id), cx).await,
        vec!["mav_task"],
        "With both .mav and .vscode tasks, only .mav tasks should appear"
    );

    register_worktree_task_used(&inventory, worktree_id, "mav_task", cx).await;
    let resolved = resolved_task_names(&inventory, Some(worktree_id), cx).await;
    assert!(
        !resolved.iter().any(|name| name == "vscode_task"),
        "Previously used .vscode tasks should not appear when .mav tasks exist, got: {resolved:?}"
    );
}
