use super::*;
use pretty_assertions::assert_eq;

#[gpui::test]
async fn test_rename_file_to_new_directory(cx: &mut gpui::TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.background_executor.clone());
    let expected_contents = "content";
    fs.as_fake()
        .insert_tree(
            "/root",
            json!({
                "test.txt": expected_contents
            }),
        )
        .await;

    let project = Project::test(fs, [path!("/root").as_ref()], cx).await;

    let (worktree, entry_id) = project.read_with(cx, |project, cx| {
        let worktree = project.worktrees(cx).next().unwrap();
        let entry_id = worktree
            .read(cx)
            .entry_for_path(rel_path("test.txt"))
            .unwrap()
            .id;
        (worktree, entry_id)
    });
    let worktree_id = worktree.read_with(cx, |worktree, _| worktree.id());
    let _result = project
        .update(cx, |project, cx| {
            project.rename_entry(
                entry_id,
                (worktree_id, rel_path("dir1/dir2/dir3/test.txt")).into(),
                cx,
            )
        })
        .await
        .unwrap();
    worktree.read_with(cx, |worktree, _| {
        assert!(
            worktree.entry_for_path(rel_path("test.txt")).is_none(),
            "Old file should have been removed"
        );
        assert!(
            worktree
                .entry_for_path(rel_path("dir1/dir2/dir3/test.txt"))
                .is_some(),
            "Whole directory hierarchy and the new file should have been created"
        );
    });
    assert_eq!(
        worktree
            .update(cx, |worktree, cx| {
                worktree.load_file(rel_path("dir1/dir2/dir3/test.txt"), cx)
            })
            .await
            .unwrap()
            .text,
        expected_contents,
        "Moved file's contents should be preserved"
    );

    let entry_id = worktree.read_with(cx, |worktree, _| {
        worktree
            .entry_for_path(rel_path("dir1/dir2/dir3/test.txt"))
            .unwrap()
            .id
    });

    let _result = project
        .update(cx, |project, cx| {
            project.rename_entry(
                entry_id,
                (worktree_id, rel_path("dir1/dir2/test.txt")).into(),
                cx,
            )
        })
        .await
        .unwrap();
    worktree.read_with(cx, |worktree, _| {
        assert!(
            worktree.entry_for_path(rel_path("test.txt")).is_none(),
            "First file should not reappear"
        );
        assert!(
            worktree
                .entry_for_path(rel_path("dir1/dir2/dir3/test.txt"))
                .is_none(),
            "Old file should have been removed"
        );
        assert!(
            worktree
                .entry_for_path(rel_path("dir1/dir2/test.txt"))
                .is_some(),
            "No error should have occurred after moving into existing directory"
        );
    });
    assert_eq!(
        worktree
            .update(cx, |worktree, cx| {
                worktree.load_file(rel_path("dir1/dir2/test.txt"), cx)
            })
            .await
            .unwrap()
            .text,
        expected_contents,
        "Moved file's contents should be preserved"
    );
}
