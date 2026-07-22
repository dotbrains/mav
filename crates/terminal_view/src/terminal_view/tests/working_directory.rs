use super::*;

// Working directory calculation tests

// No Worktrees in project -> home_dir()
#[gpui::test]
async fn no_worktree(cx: &mut TestAppContext) {
    let (project, workspace) = init_test(cx).await;
    cx.read(|cx| {
        let workspace = workspace.read(cx);
        let active_entry = project.read(cx).active_entry();

        //Make sure environment is as expected
        assert!(active_entry.is_none());
        assert!(workspace.worktrees(cx).next().is_none());

        let res = default_working_directory(workspace, cx);
        assert_eq!(res, dirs::home_dir());
        let res = first_project_directory(workspace, cx);
        assert_eq!(res, None);
    });
}

#[gpui::test]
async fn remote_no_worktree_uses_remote_shell_default_cwd(
    cx: &mut TestAppContext,
    server_cx: &mut TestAppContext,
) {
    let (_project, workspace) = init_remote_test(cx, server_cx).await;

    cx.read(|cx| {
        let workspace = workspace.read(cx);

        assert!(workspace.project().read(cx).is_remote());
        assert!(workspace.worktrees(cx).next().is_none());
        assert_eq!(default_working_directory(workspace, cx), None);
    });
}

// No active entry, but a worktree, worktree is a file -> parent directory
#[gpui::test]
async fn no_active_entry_worktree_is_file(cx: &mut TestAppContext) {
    let (project, workspace) = init_test(cx).await;

    create_file_wt(project.clone(), "/root.txt", cx).await;
    cx.read(|cx| {
        let workspace = workspace.read(cx);
        let active_entry = project.read(cx).active_entry();

        //Make sure environment is as expected
        assert!(active_entry.is_none());
        assert!(workspace.worktrees(cx).next().is_some());

        let res = default_working_directory(workspace, cx);
        assert_eq!(res, Some(Path::new("/").to_path_buf()));
        let res = first_project_directory(workspace, cx);
        assert_eq!(res, Some(Path::new("/").to_path_buf()));
    });
}

// No active entry, but a worktree, worktree is a folder -> worktree_folder
#[gpui::test]
async fn no_active_entry_worktree_is_dir(cx: &mut TestAppContext) {
    let (project, workspace) = init_test(cx).await;

    let (_wt, _entry) = create_folder_wt(project.clone(), "/root/", cx).await;
    cx.update(|cx| {
        let workspace = workspace.read(cx);
        let active_entry = project.read(cx).active_entry();

        assert!(active_entry.is_none());
        assert!(workspace.worktrees(cx).next().is_some());

        let res = default_working_directory(workspace, cx);
        assert_eq!(res, Some(Path::new("/root/").to_path_buf()));
        let res = first_project_directory(workspace, cx);
        assert_eq!(res, Some(Path::new("/root/").to_path_buf()));
    });
}

// Active entry with a work tree, worktree is a file -> worktree_folder()
#[gpui::test]
async fn active_entry_worktree_is_file(cx: &mut TestAppContext) {
    let (project, workspace) = init_test(cx).await;

    let (_wt, _entry) = create_folder_wt(project.clone(), "/root1/", cx).await;
    let (wt2, entry2) = create_file_wt(project.clone(), "/root2.txt", cx).await;
    insert_active_entry_for(wt2, entry2, project.clone(), cx);

    cx.update(|cx| {
        let workspace = workspace.read(cx);
        let active_entry = project.read(cx).active_entry();

        assert!(active_entry.is_some());

        let res = default_working_directory(workspace, cx);
        assert_eq!(res, Some(Path::new("/root1/").to_path_buf()));
        let res = first_project_directory(workspace, cx);
        assert_eq!(res, Some(Path::new("/root1/").to_path_buf()));
    });
}

// Active entry, with a worktree, worktree is a folder -> worktree_folder
#[gpui::test]
async fn active_entry_worktree_is_dir(cx: &mut TestAppContext) {
    let (project, workspace) = init_test(cx).await;

    let (_wt, _entry) = create_folder_wt(project.clone(), "/root1/", cx).await;
    let (wt2, entry2) = create_folder_wt(project.clone(), "/root2/", cx).await;
    insert_active_entry_for(wt2, entry2, project.clone(), cx);

    cx.update(|cx| {
        let workspace = workspace.read(cx);
        let active_entry = project.read(cx).active_entry();

        assert!(active_entry.is_some());

        let res = default_working_directory(workspace, cx);
        assert_eq!(res, Some(Path::new("/root2/").to_path_buf()));
        let res = first_project_directory(workspace, cx);
        assert_eq!(res, Some(Path::new("/root1/").to_path_buf()));
    });
}

// active_entry_directory: No active entry -> returns None (used by CurrentFileDirectory)
#[gpui::test]
async fn active_entry_directory_no_active_entry(cx: &mut TestAppContext) {
    let (project, _workspace) = init_test(cx).await;

    let (_wt, _entry) = create_folder_wt(project.clone(), "/root/", cx).await;

    cx.update(|cx| {
        assert!(project.read(cx).active_entry().is_none());

        let res = project.read(cx).active_entry_directory(cx);
        assert_eq!(res, None);
    });
}

// active_entry_directory: Active entry is file -> returns parent directory (used by CurrentFileDirectory)
#[gpui::test]
async fn active_entry_directory_active_file(cx: &mut TestAppContext) {
    let (project, _workspace) = init_test(cx).await;

    let (wt, _entry) = create_folder_wt(project.clone(), "/root/", cx).await;
    let entry = create_file_in_worktree(wt.clone(), "src/main.rs", cx).await;
    insert_active_entry_for(wt, entry, project.clone(), cx);

    cx.update(|cx| {
        let res = project.read(cx).active_entry_directory(cx);
        assert_eq!(res, Some(Path::new("/root/src").to_path_buf()));
    });
}

// active_entry_directory: Active entry is directory -> returns that directory (used by CurrentFileDirectory)
#[gpui::test]
async fn active_entry_directory_active_dir(cx: &mut TestAppContext) {
    let (project, _workspace) = init_test(cx).await;

    let (wt, entry) = create_folder_wt(project.clone(), "/root/", cx).await;
    insert_active_entry_for(wt, entry, project.clone(), cx);

    cx.update(|cx| {
        let res = project.read(cx).active_entry_directory(cx);
        assert_eq!(res, Some(Path::new("/root/").to_path_buf()));
    });
}
