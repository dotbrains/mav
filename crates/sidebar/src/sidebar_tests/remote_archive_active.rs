use super::*;

#[gpui::test]
async fn test_remote_archive_thread_with_active_connection(
    cx: &mut TestAppContext,
    server_cx: &mut TestAppContext,
) {
    // End-to-end test of archiving a remote thread tied to a linked git
    // worktree. Archival should:
    //  1. Persist the worktree's git state via the remote repository RPCs
    //     (head_sha / create_archive_checkpoint / update_ref).
    //  2. Remove the linked worktree directory from the *remote* filesystem
    //     via the GitRemoveWorktree RPC.
    //  3. Mark the thread metadata archived and hide it from the sidebar.
    //
    // The mock remote transport only supports one live `RemoteClient` per
    // connection at a time (each client's `start_proxy` replaces the
    // previous server channel), so we can't split the main repo and the
    // linked worktree across two remote projects the way Mav does in
    // production. Opening both as visible worktrees of a single remote
    // project still exercises every interesting path of the archive flow
    // while staying within the mock's multiplexing limits.
    init_test(cx);

    cx.update(|cx| {
        release_channel::init(semver::Version::new(0, 0, 0), cx);
    });

    let app_state = cx.update(|cx| {
        let app_state = workspace::AppState::test(cx);
        workspace::init(app_state.clone(), cx);
        app_state
    });

    server_cx.update(|cx| {
        release_channel::init(semver::Version::new(0, 0, 0), cx);
    });

    // Set up the remote filesystem with a main repo and one linked worktree.
    let server_fs = FakeFs::new(server_cx.executor());
    server_fs
        .insert_tree(
            "/project",
            serde_json::json!({
                ".git": {
                    "worktrees": {
                        "feature-a": {
                            "commondir": "../../",
                            "HEAD": "ref: refs/heads/feature-a",
                        },
                    },
                },
                "src": { "main.rs": "fn main() {}" },
            }),
        )
        .await;
    server_fs
        .insert_tree(
            "/worktrees/project/feature-a/project",
            serde_json::json!({
                ".git": "gitdir: /project/.git/worktrees/feature-a",
                "src": { "lib.rs": "// feature" },
            }),
        )
        .await;
    server_fs
        .add_linked_worktree_for_repo(
            Path::new("/project/.git"),
            false,
            git::repository::Worktree {
                path: PathBuf::from("/worktrees/project/feature-a/project"),
                ref_name: Some("refs/heads/feature-a".into()),
                sha: "abc".into(),
                is_main: false,
                is_bare: false,
            },
        )
        .await;
    server_fs.set_branch_name(Path::new("/project/.git"), Some("main"));
    server_fs.set_head_for_repo(
        Path::new("/project/.git"),
        &[("src/main.rs", "fn main() {}".into())],
        "head-sha",
    );

    // Open a single remote project with both the main repo and the linked
    // worktree as visible worktrees. The mock transport doesn't multiplex
    // multiple `RemoteClient`s over one pooled connection cleanly (each
    // client's `start_proxy` clobbers the previous one's server channel),
    // so we can't build two separate `Project::remote` instances in this
    // test. Folding both worktrees into one project still exercises the
    // archive flow's interesting paths: `build_root_plan` classifies the
    // linked worktree correctly, and `find_or_create_repository` finds
    // the main repo live on that same project — avoiding the temp-project
    // fallback that would also run into the multiplexing limitation.
    let (project, _headless, _opts) = start_remote_project(
        &server_fs,
        Path::new("/project"),
        &app_state,
        None,
        cx,
        server_cx,
    )
    .await;
    project
        .update(cx, |project, cx| {
            project.find_or_create_worktree(
                Path::new("/worktrees/project/feature-a/project"),
                true,
                cx,
            )
        })
        .await
        .expect("should open linked worktree on remote");
    project.update(cx, |p, cx| p.git_scans_complete(cx)).await;
    cx.run_until_parked();

    cx.update(|cx| <dyn fs::Fs>::set_global(app_state.fs.clone(), cx));

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let sidebar = setup_sidebar(&multi_workspace, cx);

    // The worktree thread's (main_worktree_path, folder_path) pair points
    // the folder at the linked worktree checkout and the main at the
    // parent repo, so `build_root_plan` targets the linked worktree
    // specifically and knows which main repo owns it.
    let remote_connection = project.read_with(cx, |p, cx| p.remote_connection_options(cx));

    // Record the worktree as Mav-created on the client, keyed by the remote
    // connection identity, with the creation time of the gitdir on the
    // *remote* filesystem (where the archive flow will re-stat it).
    agent_ui::test_support::record_mav_created_worktree(
        server_fs.as_ref(),
        Path::new("/worktrees/project/feature-a/project"),
        remote_connection.as_ref(),
        cx,
    )
    .await;

    let wt_thread_id = acp::SessionId::new(Arc::from("worktree-thread"));
    cx.update(|_window, cx| {
        let metadata = ThreadMetadata {
            thread_id: ThreadId::new(),
            session_id: Some(wt_thread_id.clone()),
            agent_id: agent::MAV_AGENT_ID.clone(),
            title: Some("Worktree Thread".into()),
            title_override: None,
            updated_at: chrono::TimeZone::with_ymd_and_hms(&chrono::Utc, 2024, 1, 1, 0, 0, 0)
                .unwrap(),
            created_at: None,
            interacted_at: None,
            worktree_paths: WorktreePaths::from_path_lists(
                PathList::new(&[PathBuf::from("/project")]),
                PathList::new(&[PathBuf::from("/worktrees/project/feature-a/project")]),
            )
            .unwrap(),
            archived: false,
            remote_connection,
        };
        ThreadMetadataStore::global(cx).update(cx, |store, cx| store.save(metadata, cx));
    });
    cx.run_until_parked();

    assert!(
        server_fs
            .is_dir(Path::new("/worktrees/project/feature-a/project"))
            .await,
        "linked worktree directory should exist on remote before archiving"
    );

    sidebar.update_in(cx, |sidebar: &mut Sidebar, window, cx| {
        sidebar.archive_thread(&wt_thread_id, window, cx);
    });
    cx.run_until_parked();
    server_cx.run_until_parked();

    let is_archived = cx.update(|_window, cx| {
        ThreadMetadataStore::global(cx)
            .read(cx)
            .entry_by_session(&wt_thread_id)
            .map(|t| t.archived)
            .unwrap_or(false)
    });
    assert!(is_archived, "worktree thread should be archived");

    assert!(
        !server_fs
            .is_dir(Path::new("/worktrees/project/feature-a/project"))
            .await,
        "linked worktree directory should be removed from remote fs \
         (the GitRemoveWorktree RPC runs `Repository::remove_worktree` \
         on the headless server, which deletes the directory via `Fs::remove_dir` \
         before running `git worktree remove --force`)"
    );

    let entries = visible_entries_as_strings(&sidebar, cx);
    assert!(
        !entries.iter().any(|e| e.contains("Worktree Thread")),
        "archived worktree thread should be hidden from sidebar: {entries:?}"
    );
}
