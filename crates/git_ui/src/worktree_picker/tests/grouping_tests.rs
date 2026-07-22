use super::*;

#[gpui::test]
async fn test_open_worktrees_are_grouped_under_section_header(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root"),
        json!({
            "project": {
                ".git": {},
                "file.txt": "buffer_text",
            },
            "worktrees": {},
        }),
    )
    .await;
    fs.set_head_for_repo(
        path!("/root/project/.git").as_ref(),
        &[("file.txt", "buffer_text".to_string())],
        "deadbeef",
    );

    let project = Project::test(fs.clone(), [path!("/root/project").as_ref()], cx).await;
    cx.executor().run_until_parked();

    let repository = project.read_with(cx, |project, cx| {
        project.repositories(cx).values().next().unwrap().clone()
    });
    let second_worktree_path = PathBuf::from(path!("/root/worktrees/second-wt"));

    cx.update(|cx| {
        repository.update(cx, |repository, _| {
            repository.create_worktree(
                git::repository::CreateWorktreeTarget::NewBranch {
                    branch_name: "second-wt".to_string(),
                    base_sha: Some("deadbeef".to_string()),
                },
                second_worktree_path.clone(),
            )
        })
    })
    .await
    .unwrap()
    .unwrap();

    // Open the second worktree as a visible worktree of the active project so
    // that two worktrees of the same repo are open in this window.
    project
        .update(cx, |project, cx| {
            project.create_worktree(&second_worktree_path, true, cx)
        })
        .await
        .unwrap();
    cx.executor().run_until_parked();

    let window_handle =
        cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window_handle
        .read_with(cx, |multi_workspace, _| multi_workspace.workspace().clone())
        .unwrap();

    let mut cx = VisualTestContext::from_window(window_handle.into(), cx);
    let worktree_picker = cx.update(|window, cx| {
        cx.new(|cx| WorktreePicker::new(project, workspace.downgrade(), window, cx))
    });
    cx.run_until_parked();

    let project_path = PathBuf::from(path!("/root/project"));
    worktree_picker.update(&mut cx, |worktree_picker, cx| {
            worktree_picker.picker.update(cx, |picker, _| {
                let matches = &picker.delegate.matches;

                let header_index = matches
                    .iter()
                    .position(|entry| {
                        matches!(entry, WorktreeEntry::SectionHeader(label) if label.as_ref() == "This Window")
                    })
                    .expect("section header should be present when multiple worktrees are open");

                let grouped_paths: Vec<&Path> = matches[header_index + 1..]
                    .iter()
                    .map_while(|entry| match entry {
                        WorktreeEntry::Worktree { worktree, .. } => Some(worktree.path.as_path()),
                        _ => None,
                    })
                    .collect();

                assert!(
                    grouped_paths.contains(&project_path.as_path()),
                    "main worktree should be grouped under the header"
                );
                assert!(
                    grouped_paths.contains(&second_worktree_path.as_path()),
                    "second open worktree should be grouped under the header"
                );
            })
        });
}
