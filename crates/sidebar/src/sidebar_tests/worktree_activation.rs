use super::*;

#[gpui::test]
async fn test_clicking_absorbed_worktree_thread_activates_worktree_workspace(
    cx: &mut TestAppContext,
) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    fs.insert_tree(
        "/project",
        serde_json::json!({
            ".git": {},
            "src": {},
        }),
    )
    .await;

    fs.add_linked_worktree_for_repo(
        Path::new("/project/.git"),
        false,
        git::repository::Worktree {
            path: std::path::PathBuf::from("/wt-feature-a"),
            ref_name: Some("refs/heads/feature-a".into()),
            sha: "aaa".into(),
            is_main: false,
            is_bare: false,
        },
    )
    .await;

    cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));

    let main_project = project::Project::test(fs.clone(), ["/project".as_ref()], cx).await;
    let worktree_project = project::Project::test(fs.clone(), ["/wt-feature-a".as_ref()], cx).await;

    main_project
        .update(cx, |p, cx| p.git_scans_complete(cx))
        .await;
    worktree_project
        .update(cx, |p, cx| p.git_scans_complete(cx))
        .await;

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(main_project.clone(), window, cx));

    let sidebar = setup_sidebar(&multi_workspace, cx);

    let worktree_workspace = multi_workspace.update_in(cx, |mw, window, cx| {
        mw.test_add_workspace(worktree_project.clone(), window, cx)
    });

    // Activate the main workspace before setting up the sidebar.
    let main_workspace = multi_workspace.update_in(cx, |mw, window, cx| {
        let workspace = mw.workspaces().next().unwrap().clone();
        mw.activate(workspace.clone(), None, window, cx);
        workspace
    });

    save_named_thread_metadata("thread-main", "Main Thread", &main_project, cx).await;
    save_named_thread_metadata("thread-wt", "WT Thread", &worktree_project, cx).await;

    multi_workspace.update_in(cx, |_, _window, cx| cx.notify());
    cx.run_until_parked();

    // The worktree workspace should be absorbed under the main repo.
    let entries = visible_entries_as_strings(&sidebar, cx);
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0], "v [project]");
    assert!(entries.contains(&"  Main Thread".to_string()));
    assert!(entries.contains(&"  WT Thread {wt-feature-a}".to_string()));

    let wt_thread_index = entries
        .iter()
        .position(|e| e.contains("WT Thread"))
        .expect("should find the worktree thread entry");

    assert_eq!(
        multi_workspace.read_with(cx, |mw, _| mw.workspace().clone()),
        main_workspace,
        "main workspace should be active initially"
    );

    // Focus the sidebar and select the absorbed worktree thread.
    focus_sidebar(&sidebar, cx);
    sidebar.update_in(cx, |sidebar, _window, _cx| {
        sidebar.selection = Some(wt_thread_index);
    });

    // Confirm to activate the worktree thread.
    cx.dispatch_action(Confirm);
    cx.run_until_parked();

    // The worktree workspace should now be active, not the main one.
    let active_workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());
    assert_eq!(
        active_workspace, worktree_workspace,
        "clicking an absorbed worktree thread should activate the worktree workspace"
    );
}

// Reproduces the core of the user-reported bug: a thread belonging to
// a multi-root workspace that mixes a standalone project and a linked
// git worktree can become invisible in the sidebar when its stored
// `main_worktree_paths` don't match the workspace's project group
// key. The metadata still exists and Thread History still shows it,
// but the sidebar rebuild's lookups all miss.
//
// Real-world setup: a single multi-root workspace whose roots are
// `[/cloud, /worktrees/mav/wt_a/mav]`, where:
//   - `/cloud` is a standalone git repo (main == folder).
//   - `/worktrees/mav/wt_a/mav` is a linked worktree of `/mav`.
//
// Once git scans complete the project group key is
// `[/cloud, /mav]` — the main paths of the two roots. A thread
// created in this workspace is written with
// `main=[/cloud, /mav], folder=[/cloud, /worktrees/mav/wt_a/mav]`
// and the sidebar finds it via `entries_for_main_worktree_path`.
//
// If some other code path (stale data on reload, a path-less archive
// restored via the project picker, a legacy write …) persists the
// thread with `main == folder` instead, the stored
// `main_worktree_paths` is
// `[/cloud, /worktrees/mav/wt_a/mav]` ≠ `[/cloud, /mav]`. The three
// lookups in `rebuild_contents` all miss:
//
//   1. `entries_for_main_worktree_path([/cloud, /mav])` — the
//      thread's stored main doesn't equal the group key.
//   2. `entries_for_path([/cloud, /mav])` — the thread's folder paths
//      don't equal the group key either.
//   3. The linked-worktree fallback iterates the group's workspaces'
//      `linked_worktrees()` snapshots. Those yield *sibling* linked
//      worktrees of the repo, not the workspace's own roots, so the
//      thread's folder `/worktrees/mav/wt_a/mav` doesn't match.
//
// The row falls out of the sidebar entirely — matching the user's
// symptom of a thread visible in the agent panel but missing from
// the sidebar. It only reappears once something re-writes the
// thread's metadata in the good shape (e.g. `handle_conversation_event`
// firing after the user sends a message).
//
// We directly persist the bad shape via `store.save(...)` rather
// than trying to reproduce the original writer. The bug is
// ultimately about the sidebar's tolerance for any stale row whose
// folder paths correspond to an open workspace's roots, regardless
// of how that row came to be in the store.
#[gpui::test]
async fn test_sidebar_keeps_multi_root_thread_with_stale_main_paths(cx: &mut TestAppContext) {
    agent_ui::test_support::init_test(cx);
    cx.update(|cx| {
        cx.set_global(agent_ui::MaxIdleRetainedThreads(1));
        ThreadStore::init_global(cx);
        ThreadMetadataStore::init_global(cx);
        language_model::LanguageModelRegistry::test(cx);
        prompt_store::init(cx);
    });

    let fs = FakeFs::new(cx.executor());

    // Standalone repo — one of the workspace's two roots, main
    // worktree of its own .git.
    fs.insert_tree(
        "/cloud",
        serde_json::json!({
            ".git": {},
            "src": {},
        }),
    )
    .await;

    // Separate /mav repo whose linked worktree will form the second
    // workspace root. /mav itself is NOT opened as a workspace root.
    fs.insert_tree(
        "/mav",
        serde_json::json!({
            ".git": {},
            "src": {},
        }),
    )
    .await;
    fs.insert_tree(
        "/worktrees/mav/wt_a/mav",
        serde_json::json!({
            ".git": "gitdir: /mav/.git/worktrees/wt_a",
            "src": {},
        }),
    )
    .await;
    fs.add_linked_worktree_for_repo(
        Path::new("/mav/.git"),
        false,
        git::repository::Worktree {
            path: std::path::PathBuf::from("/worktrees/mav/wt_a/mav"),
            ref_name: Some("refs/heads/wt_a".into()),
            sha: "aaa".into(),
            is_main: false,
            is_bare: false,
        },
    )
    .await;

    cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));

    // Single multi-root project with both /cloud and the linked
    // worktree of /mav.
    let project = project::Project::test(
        fs.clone(),
        ["/cloud".as_ref(), "/worktrees/mav/wt_a/mav".as_ref()],
        cx,
    )
    .await;
    project.update(cx, |p, cx| p.git_scans_complete(cx)).await;

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let sidebar = setup_sidebar(&multi_workspace, cx);
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspaces().next().unwrap().clone());
    let _panel = add_agent_panel(&workspace, cx);
    cx.run_until_parked();

    // Sanity-check the shapes the rest of the test depends on.
    let group_key = workspace.read_with(cx, |ws, cx| ws.project_group_key(cx));
    let expected_main_paths = PathList::new(&[PathBuf::from("/cloud"), PathBuf::from("/mav")]);
    assert_eq!(
        group_key.path_list(),
        &expected_main_paths,
        "expected the multi-root workspace's project group key to normalize to \
         [/cloud, /mav] (main of the standalone repo + main of the linked worktree)"
    );

    let folder_paths = PathList::new(&[
        PathBuf::from("/cloud"),
        PathBuf::from("/worktrees/mav/wt_a/mav"),
    ]);
    let workspace_root_paths = workspace.read_with(cx, |ws, cx| PathList::new(&ws.root_paths(cx)));
    assert_eq!(
        workspace_root_paths, folder_paths,
        "expected the workspace's root paths to equal [/cloud, /worktrees/mav/wt_a/mav]"
    );

    let session_id = acp::SessionId::new(Arc::from("multi-root-stale-paths"));
    let thread_id = ThreadId::new();

    // Persist the thread in the "bad" shape that the bug manifests as:
    // main == folder for every root. Any stale row where
    // `main_worktree_paths` no longer equals the group key produces
    // the same user-visible symptom; this is the concrete shape
    // produced by `WorktreePaths::from_folder_paths` on the workspace
    // roots.
    cx.update(|_, cx| {
        ThreadMetadataStore::global(cx).update(cx, |store, cx| {
            store.save(
                ThreadMetadata {
                    thread_id,
                    session_id: Some(session_id.clone()),
                    agent_id: agent::MAV_AGENT_ID.clone(),
                    title: Some("Stale Multi-Root Thread".into()),
                    title_override: None,
                    updated_at: Utc::now(),
                    created_at: None,
                    interacted_at: None,
                    worktree_paths: WorktreePaths::from_folder_paths(&folder_paths),
                    archived: false,
                    remote_connection: None,
                },
                cx,
            )
        });
    });
    cx.run_until_parked();

    let entries = visible_entries_as_strings(&sidebar, cx);
    let visible = sidebar.read_with(cx, |sidebar, _cx| has_thread_entry(sidebar, &session_id));

    // If this assert fails, we've reproduced the bug: the sidebar's
    // rebuild queries can't locate the thread under the current
    // project group, even though the metadata is intact and the
    // thread's folder paths exactly equal the open workspace's roots.
    assert!(
        visible,
        "thread disappeared from the sidebar when its main_worktree_paths \
         ({folder_paths:?}) diverged from the project group key ({expected_main_paths:?}); \
         sidebar entries: {entries:?}"
    );
}
