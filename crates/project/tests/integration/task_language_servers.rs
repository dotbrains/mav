use super::*;
use pretty_assertions::assert_eq;

#[gpui::test]
async fn test_invalid_local_tasks_shows_toast_with_doc_link(cx: &mut gpui::TestAppContext) {
    init_test(cx);
    TaskStore::init(None);

    // We need to start with a valid `.mav/tasks.json` file as otherwise the
    // event is emitted before we havd a chance to setup the event subscription.
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/dir"),
        json!({
            ".mav": {
                "tasks.json": r#"[{ "label": "valid task", "command": "echo" }]"#,
            },
            "file.rs": ""
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let saw_toast = Rc::new(RefCell::new(false));

    // Update the `.mav/tasks.json` file with an invalid variable, so we can
    // later assert that the `Event::Toast` even is emitted.
    fs.save(
        path!("/dir/.mav/tasks.json").as_ref(),
        &r#"[{ "label": "test $MAV_FOO", "command": "echo" }]"#.into(),
        Default::default(),
    )
    .await
    .unwrap();

    project.update(cx, |_, cx| {
        let saw_toast = saw_toast.clone();

        cx.subscribe(&project, move |_, _, event: &Event, _| match event {
            Event::Toast {
                notification_id,
                message,
                link: Some(ToastLink { url, .. }),
            } => {
                assert!(notification_id.starts_with("local-tasks-"));
                assert!(message.contains("MAV_FOO"));
                assert_eq!(*url, "https://mav.dev/docs/tasks");
                *saw_toast.borrow_mut() = true;
            }
            _ => {}
        })
        .detach();
    });

    cx.run_until_parked();
    assert!(
        *saw_toast.borrow(),
        "Expected `Event::Toast` was never emitted"
    );
}

#[gpui::test]
async fn test_fallback_to_single_worktree_tasks(cx: &mut gpui::TestAppContext) {
    init_test(cx);
    TaskStore::init(None);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/dir"),
        json!({
            ".mav": {
                "tasks.json": r#"[{
                    "label": "test worktree root",
                    "command": "echo $MAV_WORKTREE_ROOT"
                }]"#,
            },
            "a": {
                "a.rs": "fn a() {\n    A\n}"
            },
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let _worktree = project.update(cx, |project, cx| project.worktrees(cx).next().unwrap());

    cx.executor().run_until_parked();
    let worktree_id = cx.update(|cx| {
        project.update(cx, |project, cx| {
            project.worktrees(cx).next().unwrap().read(cx).id()
        })
    });

    let active_non_worktree_item_tasks = cx
        .update(|cx| {
            get_all_tasks(
                &project,
                Arc::new(TaskContexts {
                    active_item_context: Some((Some(worktree_id), None, TaskContext::default())),
                    active_worktree_context: None,
                    other_worktree_contexts: Vec::new(),
                    lsp_task_sources: HashMap::default(),
                    latest_selection: None,
                }),
                cx,
            )
        })
        .await;
    assert!(
        active_non_worktree_item_tasks.is_empty(),
        "A task can not be resolved with context with no MAV_WORKTREE_ROOT data"
    );

    let active_worktree_tasks = cx
        .update(|cx| {
            get_all_tasks(
                &project,
                Arc::new(TaskContexts {
                    active_item_context: Some((Some(worktree_id), None, TaskContext::default())),
                    active_worktree_context: Some((worktree_id, {
                        let mut worktree_context = TaskContext::default();
                        worktree_context
                            .task_variables
                            .insert(task::VariableName::WorktreeRoot, "/dir".to_string());
                        worktree_context
                    })),
                    other_worktree_contexts: Vec::new(),
                    lsp_task_sources: HashMap::default(),
                    latest_selection: None,
                }),
                cx,
            )
        })
        .await;
    assert_eq!(
        active_worktree_tasks
            .into_iter()
            .map(|(source_kind, task)| {
                let resolved = task.resolved;
                (source_kind, resolved.command.unwrap())
            })
            .collect::<Vec<_>>(),
        vec![(
            TaskSourceKind::Worktree {
                id: worktree_id,
                directory_in_worktree: rel_path(".mav").into(),
                id_base: "local worktree tasks from directory \".mav\"".into(),
            },
            "echo /dir".to_string(),
        )]
    );
}

#[gpui::test]
async fn test_running_multiple_instances_of_a_single_server_in_one_worktree(
    cx: &mut gpui::TestAppContext,
) {
    pub(crate) struct PyprojectTomlManifestProvider;

    impl ManifestProvider for PyprojectTomlManifestProvider {
        fn name(&self) -> ManifestName {
            SharedString::new_static("pyproject.toml").into()
        }

        fn search(
            &self,
            ManifestQuery {
                path,
                depth,
                delegate,
            }: ManifestQuery,
        ) -> Option<Arc<RelPath>> {
            const WORKSPACE_LOCKFILES: &[&str] =
                &["uv.lock", "poetry.lock", "pdm.lock", "Pipfile.lock"];

            let mut innermost_pyproject = None;
            let mut outermost_workspace_root = None;

            for path in path.ancestors().take(depth) {
                let pyproject_path = path.join(rel_path("pyproject.toml"));
                if delegate.exists(&pyproject_path, Some(false)) {
                    if innermost_pyproject.is_none() {
                        innermost_pyproject = Some(Arc::from(path));
                    }

                    let has_lockfile = WORKSPACE_LOCKFILES.iter().any(|lockfile| {
                        let lockfile_path = path.join(rel_path(lockfile));
                        delegate.exists(&lockfile_path, Some(false))
                    });
                    if has_lockfile {
                        outermost_workspace_root = Some(Arc::from(path));
                    }
                }
            }

            outermost_workspace_root.or(innermost_pyproject)
        }
    }

    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    fs.insert_tree(
        path!("/the-root"),
        json!({
            ".mav": {
                "settings.json": r#"
                {
                    "languages": {
                        "Python": {
                            "language_servers": ["ty"]
                        }
                    }
                }"#
            },
            "project-a": {
                ".venv": {},
                "file.py": "",
                "pyproject.toml": ""
            },
            "project-b": {
                ".venv": {},
                "source_file.py":"",
                "another_file.py": "",
                "pyproject.toml": ""
            }
        }),
    )
    .await;
    cx.update(|cx| {
        ManifestProvidersStore::global(cx).register(Arc::new(PyprojectTomlManifestProvider))
    });

    let project = Project::test(fs.clone(), [path!("/the-root").as_ref()], cx).await;
    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    let _fake_python_server = language_registry.register_fake_lsp(
        "Python",
        FakeLspAdapter {
            name: "ty",
            capabilities: lsp::ServerCapabilities {
                ..Default::default()
            },
            ..Default::default()
        },
    );

    language_registry.add(python_lang(fs.clone()));
    let (first_buffer, _handle) = project
        .update(cx, |project, cx| {
            project.open_local_buffer_with_lsp(path!("/the-root/project-a/file.py"), cx)
        })
        .await
        .unwrap();
    cx.executor().run_until_parked();
    let servers = project.update(cx, |project, cx| {
        project.lsp_store().update(cx, |this, cx| {
            first_buffer.update(cx, |buffer, cx| {
                this.running_language_servers_for_local_buffer(buffer, cx)
                    .map(|(adapter, server)| (adapter.clone(), server.clone()))
                    .collect::<Vec<_>>()
            })
        })
    });
    cx.executor().run_until_parked();
    assert_eq!(servers.len(), 1);
    let (adapter, server) = servers.into_iter().next().unwrap();
    assert_eq!(adapter.name(), LanguageServerName::new_static("ty"));
    assert_eq!(server.server_id(), LanguageServerId(0));
    // `workspace_folders` are set to the rooting point.
    assert_eq!(
        server.workspace_folders(),
        BTreeSet::from_iter(
            [Uri::from_file_path(path!("/the-root/project-a")).unwrap()].into_iter()
        )
    );

    let (second_project_buffer, _other_handle) = project
        .update(cx, |project, cx| {
            project.open_local_buffer_with_lsp(path!("/the-root/project-b/source_file.py"), cx)
        })
        .await
        .unwrap();
    cx.executor().run_until_parked();
    let servers = project.update(cx, |project, cx| {
        project.lsp_store().update(cx, |this, cx| {
            second_project_buffer.update(cx, |buffer, cx| {
                this.running_language_servers_for_local_buffer(buffer, cx)
                    .map(|(adapter, server)| (adapter.clone(), server.clone()))
                    .collect::<Vec<_>>()
            })
        })
    });
    cx.executor().run_until_parked();
    assert_eq!(servers.len(), 1);
    let (adapter, server) = servers.into_iter().next().unwrap();
    assert_eq!(adapter.name(), LanguageServerName::new_static("ty"));
    // We're not using venvs at all here, so both folders should fall under the same root.
    assert_eq!(server.server_id(), LanguageServerId(0));
    // Now, let's select a different toolchain for one of subprojects.

    let Toolchains {
        toolchains: available_toolchains_for_b,
        root_path,
        ..
    } = project
        .update(cx, |this, cx| {
            let worktree_id = this.worktrees(cx).next().unwrap().read(cx).id();
            this.available_toolchains(
                ProjectPath {
                    worktree_id,
                    path: rel_path("project-b/source_file.py").into(),
                },
                LanguageName::new_static("Python"),
                cx,
            )
        })
        .await
        .expect("A toolchain to be discovered");
    assert_eq!(root_path.as_ref(), rel_path("project-b"));
    assert_eq!(available_toolchains_for_b.toolchains().len(), 1);
    let currently_active_toolchain = project
        .update(cx, |this, cx| {
            let worktree_id = this.worktrees(cx).next().unwrap().read(cx).id();
            this.active_toolchain(
                ProjectPath {
                    worktree_id,
                    path: rel_path("project-b/source_file.py").into(),
                },
                LanguageName::new_static("Python"),
                cx,
            )
        })
        .await;

    assert!(currently_active_toolchain.is_none());
    let _ = project
        .update(cx, |this, cx| {
            let worktree_id = this.worktrees(cx).next().unwrap().read(cx).id();
            this.activate_toolchain(
                ProjectPath {
                    worktree_id,
                    path: root_path,
                },
                available_toolchains_for_b
                    .toolchains
                    .into_iter()
                    .next()
                    .unwrap(),
                cx,
            )
        })
        .await
        .unwrap();
    cx.run_until_parked();
    let servers = project.update(cx, |project, cx| {
        project.lsp_store().update(cx, |this, cx| {
            second_project_buffer.update(cx, |buffer, cx| {
                this.running_language_servers_for_local_buffer(buffer, cx)
                    .map(|(adapter, server)| (adapter.clone(), server.clone()))
                    .collect::<Vec<_>>()
            })
        })
    });
    cx.executor().run_until_parked();
    assert_eq!(servers.len(), 1);
    let (adapter, server) = servers.into_iter().next().unwrap();
    assert_eq!(adapter.name(), LanguageServerName::new_static("ty"));
    // There's a new language server in town.
    assert_eq!(server.server_id(), LanguageServerId(1));
}
