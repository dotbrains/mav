use super::*;
use crate::thread_metadata_store::ThreadMetadataStore;

#[gpui::test]
async fn test_work_dirs_update_when_worktrees_change(cx: &mut TestAppContext) {
    init_test(cx);
    cx.update(|cx| {
        agent::ThreadStore::init_global(cx);
        language_model::LanguageModelRegistry::test(cx);
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/project_a", json!({ "file.txt": "" }))
        .await;
    let project = Project::test(fs.clone(), [Path::new("/project_a")], cx).await;

    let multi_workspace =
        cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace
        .read_with(cx, |mw, _cx| mw.workspace().clone())
        .unwrap();
    let mut cx = VisualTestContext::from_window(multi_workspace.into(), cx);

    let panel = workspace.update_in(&mut cx, |workspace, window, cx| {
        cx.new(|cx| AgentPanel::new(workspace, window, cx))
    });

    let connection_a = StubAgentConnection::new().with_agent_id("agent-a".into());
    open_thread_with_custom_connection(&panel, connection_a.clone(), &mut cx);
    send_message(&panel, &mut cx);
    let session_id_a = active_session_id(&panel, &cx);
    let thread_id_a = active_thread_id(&panel, &cx);

    let connection_c = StubAgentConnection::new().with_agent_id("agent-c".into());
    connection_c.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
        acp::ContentChunk::new("done".into()),
    )]);
    open_thread_with_custom_connection(&panel, connection_c.clone(), &mut cx);
    send_message(&panel, &mut cx);
    let thread_id_c = active_thread_id(&panel, &cx);

    let connection_b = StubAgentConnection::new().with_agent_id("agent-b".into());
    open_thread_with_custom_connection(&panel, connection_b.clone(), &mut cx);
    send_message(&panel, &mut cx);
    let session_id_b = active_session_id(&panel, &cx);
    let _thread_id_b = active_thread_id(&panel, &cx);

    let metadata_store = cx.update(|_, cx| ThreadMetadataStore::global(cx));

    panel.read_with(&cx, |panel, _cx| {
        assert!(
            panel.retained_threads.contains_key(&thread_id_a),
            "Thread A should be in retained_threads"
        );
        assert!(
            panel.retained_threads.contains_key(&thread_id_c),
            "Thread C should be in retained_threads"
        );
    });

    let initial_b_paths = panel.read_with(&cx, |panel, cx| {
        let thread = panel.active_agent_thread(cx).unwrap();
        thread.read(cx).work_dirs().cloned().unwrap()
    });
    assert_eq!(
        initial_b_paths.ordered_paths().collect::<Vec<_>>(),
        vec![&PathBuf::from("/project_a")],
        "Thread B should initially have only /project_a"
    );

    fs.insert_tree("/project_b", json!({ "other.txt": "" }))
        .await;
    let (new_tree, _) = project
        .update(&mut cx, |project, cx| {
            project.find_or_create_worktree("/project_b", true, cx)
        })
        .await
        .unwrap();
    cx.read(|cx| new_tree.read(cx).as_local().unwrap().scan_complete())
        .await;
    cx.run_until_parked();

    let updated_b_paths = panel.read_with(&cx, |panel, cx| {
        let thread = panel.active_agent_thread(cx).unwrap();
        thread.read(cx).work_dirs().cloned().unwrap()
    });
    let mut b_paths_sorted = updated_b_paths.ordered_paths().cloned().collect::<Vec<_>>();
    b_paths_sorted.sort();
    assert_eq!(
        b_paths_sorted,
        vec![PathBuf::from("/project_a"), PathBuf::from("/project_b")],
        "Thread B work_dirs should include both worktrees after adding /project_b"
    );

    let updated_a_paths = panel.read_with(&cx, |panel, cx| {
        let bg_view = panel.retained_threads.get(&thread_id_a).unwrap();
        let root_thread = bg_view.read(cx).root_thread_view().unwrap();
        root_thread
            .read(cx)
            .thread
            .read(cx)
            .work_dirs()
            .cloned()
            .unwrap()
    });
    let mut a_paths_sorted = updated_a_paths.ordered_paths().cloned().collect::<Vec<_>>();
    a_paths_sorted.sort();
    assert_eq!(
        a_paths_sorted,
        vec![PathBuf::from("/project_a"), PathBuf::from("/project_b")],
        "Thread A work_dirs should include both worktrees after adding /project_b"
    );

    let updated_c_paths = panel.read_with(&cx, |panel, cx| {
        let bg_view = panel.retained_threads.get(&thread_id_c).unwrap();
        let root_thread = bg_view.read(cx).root_thread_view().unwrap();
        root_thread
            .read(cx)
            .thread
            .read(cx)
            .work_dirs()
            .cloned()
            .unwrap()
    });
    let mut c_paths_sorted = updated_c_paths.ordered_paths().cloned().collect::<Vec<_>>();
    c_paths_sorted.sort();
    assert_eq!(
        c_paths_sorted,
        vec![PathBuf::from("/project_a"), PathBuf::from("/project_b")],
        "Thread C (idle background) work_dirs should include both worktrees after adding /project_b"
    );

    cx.run_until_parked();
    for (label, session_id) in [("thread B", &session_id_b), ("thread A", &session_id_a)] {
        let metadata_paths = metadata_store.read_with(&cx, |store, _cx| {
            let metadata = store
                .entry_by_session(session_id)
                .unwrap_or_else(|| panic!("{label} thread metadata should exist"));
            metadata.folder_paths().clone()
        });
        let mut sorted = metadata_paths.ordered_paths().cloned().collect::<Vec<_>>();
        sorted.sort();
        assert_eq!(
            sorted,
            vec![PathBuf::from("/project_a"), PathBuf::from("/project_b")],
            "{label} thread metadata folder_paths should include both worktrees"
        );
    }

    let worktree_b_id = new_tree.read_with(&cx, |tree, _| tree.id());
    project.update(&mut cx, |project, cx| {
        project.remove_worktree(worktree_b_id, cx);
    });
    cx.run_until_parked();

    let after_remove_b = panel.read_with(&cx, |panel, cx| {
        let thread = panel.active_agent_thread(cx).unwrap();
        thread.read(cx).work_dirs().cloned().unwrap()
    });
    assert_eq!(
        after_remove_b.ordered_paths().collect::<Vec<_>>(),
        vec![&PathBuf::from("/project_a")],
        "Thread B work_dirs should revert to only /project_a after removing /project_b"
    );

    let after_remove_a = panel.read_with(&cx, |panel, cx| {
        let bg_view = panel.retained_threads.get(&thread_id_a).unwrap();
        let root_thread = bg_view.read(cx).root_thread_view().unwrap();
        root_thread
            .read(cx)
            .thread
            .read(cx)
            .work_dirs()
            .cloned()
            .unwrap()
    });
    assert_eq!(
        after_remove_a.ordered_paths().collect::<Vec<_>>(),
        vec![&PathBuf::from("/project_a")],
        "Thread A work_dirs should revert to only /project_a after removing /project_b"
    );
}
