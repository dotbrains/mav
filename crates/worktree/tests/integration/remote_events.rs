use super::*;
use pretty_assertions::assert_eq;

#[gpui::test]
async fn test_remote_worktree_without_git_emits_root_repo_event_after_first_update(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        let store = SettingsStore::test(cx);
        cx.set_global(store);
    });

    let client = AnyProtoClient::new(NoopProtoClient::new());

    let worktree = cx.update(|cx| {
        Worktree::remote(
            1,
            clock::ReplicaId::new(1),
            proto::WorktreeMetadata {
                id: 1,
                root_name: "project".to_string(),
                visible: true,
                abs_path: "/home/user/project".to_string(),
                root_repo_common_dir: None,
            },
            client,
            PathStyle::Posix,
            cx,
        )
    });

    let events: Arc<std::sync::Mutex<Vec<&'static str>>> =
        Arc::new(std::sync::Mutex::new(Vec::new()));
    let events_clone = events.clone();
    cx.update(|cx| {
        cx.subscribe(&worktree, move |_, event, _cx| {
            if matches!(event, Event::UpdatedRootRepoCommonDir { .. }) {
                events_clone
                    .lock()
                    .unwrap()
                    .push("UpdatedRootRepoCommonDir");
            }
            if matches!(event, Event::UpdatedEntries(_)) {
                events_clone.lock().unwrap().push("UpdatedEntries");
            }
        })
        .detach();
    });

    // Send an update with entries but no repo info (plain directory).
    worktree.update(cx, |worktree, _cx| {
        worktree
            .as_remote()
            .unwrap()
            .update_from_remote(proto::UpdateWorktree {
                project_id: 1,
                worktree_id: 1,
                abs_path: "/home/user/project".to_string(),
                root_name: "project".to_string(),
                updated_entries: vec![proto::Entry {
                    id: 1,
                    is_dir: true,
                    path: "".to_string(),
                    inode: 1,
                    mtime: Some(proto::Timestamp {
                        seconds: 0,
                        nanos: 0,
                    }),
                    is_ignored: false,
                    is_hidden: false,
                    is_external: false,
                    is_fifo: false,
                    size: None,
                    canonical_path: None,
                }],
                removed_entries: vec![],
                scan_id: 1,
                is_last_update: true,
                updated_repositories: vec![],
                removed_repositories: vec![],
                root_repo_common_dir: None,
            });
    });

    cx.run_until_parked();

    let fired = events.lock().unwrap();
    assert!(
        fired.contains(&"UpdatedEntries"),
        "UpdatedEntries should fire after remote update"
    );
    assert!(
        fired.contains(&"UpdatedRootRepoCommonDir"),
        "UpdatedRootRepoCommonDir should fire after first remote update even when \
         root_repo_common_dir is None, to signal that repo state is now known"
    );
}

#[gpui::test]
async fn test_remote_worktree_with_git_emits_root_repo_event_when_repo_info_arrives(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        let store = SettingsStore::test(cx);
        cx.set_global(store);
    });

    let client = AnyProtoClient::new(NoopProtoClient::new());

    let worktree = cx.update(|cx| {
        Worktree::remote(
            1,
            clock::ReplicaId::new(1),
            proto::WorktreeMetadata {
                id: 1,
                root_name: "project".to_string(),
                visible: true,
                abs_path: "/home/user/project".to_string(),
                root_repo_common_dir: None,
            },
            client,
            PathStyle::Posix,
            cx,
        )
    });

    let events: Arc<std::sync::Mutex<Vec<&'static str>>> =
        Arc::new(std::sync::Mutex::new(Vec::new()));
    let events_clone = events.clone();
    cx.update(|cx| {
        cx.subscribe(&worktree, move |_, event, _cx| {
            if matches!(event, Event::UpdatedRootRepoCommonDir { .. }) {
                events_clone
                    .lock()
                    .unwrap()
                    .push("UpdatedRootRepoCommonDir");
            }
        })
        .detach();
    });

    // Send an update where repo info arrives (None -> Some).
    worktree.update(cx, |worktree, _cx| {
        worktree
            .as_remote()
            .unwrap()
            .update_from_remote(proto::UpdateWorktree {
                project_id: 1,
                worktree_id: 1,
                abs_path: "/home/user/project".to_string(),
                root_name: "project".to_string(),
                updated_entries: vec![proto::Entry {
                    id: 1,
                    is_dir: true,
                    path: "".to_string(),
                    inode: 1,
                    mtime: Some(proto::Timestamp {
                        seconds: 0,
                        nanos: 0,
                    }),
                    is_ignored: false,
                    is_hidden: false,
                    is_external: false,
                    is_fifo: false,
                    size: None,
                    canonical_path: None,
                }],
                removed_entries: vec![],
                scan_id: 1,
                is_last_update: true,
                updated_repositories: vec![],
                removed_repositories: vec![],
                root_repo_common_dir: Some("/home/user/project/.git".to_string()),
            });
    });

    cx.run_until_parked();

    let fired = events.lock().unwrap();
    assert!(
        fired.contains(&"UpdatedRootRepoCommonDir"),
        "UpdatedRootRepoCommonDir should fire when repo info arrives (None -> Some)"
    );
    assert_eq!(
        fired
            .iter()
            .filter(|e| **e == "UpdatedRootRepoCommonDir")
            .count(),
        1,
        "should fire exactly once, not duplicate"
    );
}

// Regression test: a remote worktree used to emit `UpdatedEntries` with an
// empty changeset (`Arc::default()`), discarding the changed paths. Consumers
// that key off those paths - notably the agent's `.agents/skills` refresh -
// therefore never fired on remote projects, so skills pasted into an already
// open project were never picked up. The changeset must carry the real paths.
//
// This drives the real host -> remote pipeline: a `FakeFs`-backed local
// worktree scans the filesystem and produces `UpdateWorktree` messages via
// `observe_updates`, which we relay into a remote worktree exactly as the
// collab server does.
#[gpui::test]
async fn test_remote_worktree_update_entries_carry_changed_paths(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        path!("/root"),
        json!({
            ".agents": {
                "skills": {}
            }
        }),
    )
    .await;

    // The host worktree scans the fake filesystem and broadcasts updates.
    let host = Worktree::local(
        path!("/root").as_ref(),
        true,
        fs.clone(),
        Default::default(),
        true,
        WorktreeId::from_proto(1),
        &mut cx.to_async(),
    )
    .await
    .unwrap();
    cx.read(|cx| host.read(cx).as_local().unwrap().scan_complete())
        .await;

    // The remote worktree receives those updates over a simulated connection.
    let remote = cx.update(|cx| {
        Worktree::remote(
            1,
            clock::ReplicaId::new(1),
            proto::WorktreeMetadata {
                id: 1,
                root_name: "root".to_string(),
                visible: true,
                abs_path: path!("/root").to_string(),
                root_repo_common_dir: None,
            },
            AnyProtoClient::new(NoopProtoClient::new()),
            PathStyle::local(),
            cx,
        )
    });

    // Relay every `UpdateWorktree` the host emits into the remote worktree,
    // mirroring how the collab server forwards them. The callback only buffers
    // the messages; we apply them on the foreground via `relay`.
    let pending: Arc<Mutex<Vec<proto::UpdateWorktree>>> = Arc::new(Mutex::new(Vec::new()));
    host.update(cx, |host, cx| {
        let pending = pending.clone();
        host.as_local_mut()
            .unwrap()
            .observe_updates(1, cx, move |update| {
                pending.lock().push(update);
                async { true }
            });
    });
    let relay = {
        let remote = remote.clone();
        move |cx: &mut TestAppContext| {
            let updates = std::mem::take(&mut *pending.lock());
            remote.update(cx, |remote, _| {
                let remote = remote.as_remote().unwrap();
                for update in updates {
                    remote.update_from_remote(update);
                }
            });
        }
    };

    // Record the (path, change) pairs from every `UpdatedEntries` event the
    // remote worktree emits.
    let changes: Arc<Mutex<Vec<(String, PathChange)>>> = Arc::new(Mutex::new(Vec::new()));
    cx.update(|cx| {
        let changes = changes.clone();
        cx.subscribe(&remote, move |_, event, _cx| {
            if let Event::UpdatedEntries(updated) = event {
                changes.lock().extend(
                    updated
                        .iter()
                        .map(|(path, _, change)| (path.as_unix_str().to_string(), *change)),
                );
            }
        })
        .detach();
    });

    // Flush the initial sync (root + existing dirs) and ignore those paths.
    cx.run_until_parked();
    relay(cx);
    cx.run_until_parked();
    changes.lock().clear();

    // Paste a skill folder into `.agents/skills` on the host.
    fs.insert_tree(
        path!("/root/.agents/skills/skill-1"),
        json!({ "SKILL.md": "skill" }),
    )
    .await;
    cx.run_until_parked();
    relay(cx);
    cx.run_until_parked();

    {
        let changes = changes.lock();
        assert!(
            changes
                .iter()
                .any(|(path, change)| path == ".agents/skills/skill-1/SKILL.md"
                    && *change == PathChange::AddedOrUpdated),
            "remote UpdatedEntries should carry the added skill path, got {:?}",
            changes
        );
    }
    changes.lock().clear();

    // Remove the skill folder. The wire format only carries entry ids for
    // removals, so the remote worktree must resolve their paths against the
    // previous snapshot before it is replaced.
    fs.remove_dir(
        path!("/root/.agents/skills/skill-1").as_ref(),
        RemoveOptions {
            recursive: true,
            ignore_if_not_exists: false,
        },
    )
    .await
    .unwrap();
    cx.run_until_parked();
    relay(cx);
    cx.run_until_parked();

    let changes = changes.lock();
    assert!(
        changes
            .iter()
            .any(|(path, change)| path == ".agents/skills/skill-1/SKILL.md"
                && *change == PathChange::Removed),
        "remote UpdatedEntries should carry removed paths resolved from the \
         previous snapshot, got {:?}",
        changes
    );
}
