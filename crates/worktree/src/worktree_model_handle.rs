use super::*;

pub trait WorktreeModelHandle {
    #[cfg(feature = "test-support")]
    fn flush_fs_events<'a>(
        &self,
        cx: &'a mut gpui::TestAppContext,
    ) -> futures::future::LocalBoxFuture<'a, ()>;

    #[cfg(feature = "test-support")]
    fn flush_fs_events_in_root_git_repository<'a>(
        &self,
        cx: &'a mut gpui::TestAppContext,
    ) -> futures::future::LocalBoxFuture<'a, ()>;
}

impl WorktreeModelHandle for Entity<Worktree> {
    // When the worktree's FS event stream sometimes delivers "redundant" events for FS changes that
    // occurred before the worktree was constructed. These events can cause the worktree to perform
    // extra directory scans, and emit extra scan-state notifications.
    //
    // This function mutates the worktree's directory and waits for those mutations to be picked up,
    // to ensure that all redundant FS events have already been processed.
    #[cfg(feature = "test-support")]
    fn flush_fs_events<'a>(
        &self,
        cx: &'a mut gpui::TestAppContext,
    ) -> futures::future::LocalBoxFuture<'a, ()> {
        let file_name = "fs-event-sentinel";

        let tree = self.clone();
        let (fs, root_path) = self.read_with(cx, |tree, _| {
            let tree = tree.as_local().unwrap();
            (tree.fs.clone(), tree.abs_path.clone())
        });

        async move {
            // Subscribe to events BEFORE creating the file to avoid race condition
            // where events fire before subscription is set up
            let mut events = cx.events(&tree);

            fs.create_file(&root_path.join(file_name), Default::default())
                .await
                .unwrap();

            // Check if condition is already met before waiting for events
            let file_exists = || {
                tree.read_with(cx, |tree, _| {
                    tree.entry_for_path(RelPath::unix(file_name).unwrap())
                        .is_some()
                })
            };

            // Use select to avoid blocking indefinitely if events are delayed
            while !file_exists() {
                futures::select_biased! {
                    _ = events.next() => {}
                    _ = futures::FutureExt::fuse(cx.background_executor.timer(std::time::Duration::from_millis(10))) => {}
                }
            }

            fs.remove_file(&root_path.join(file_name), Default::default())
                .await
                .unwrap();

            // Check if condition is already met before waiting for events
            let file_gone = || {
                tree.read_with(cx, |tree, _| {
                    tree.entry_for_path(RelPath::unix(file_name).unwrap())
                        .is_none()
                })
            };

            // Use select to avoid blocking indefinitely if events are delayed
            while !file_gone() {
                futures::select_biased! {
                    _ = events.next() => {}
                    _ = futures::FutureExt::fuse(cx.background_executor.timer(std::time::Duration::from_millis(10))) => {}
                }
            }

            cx.update(|cx| tree.read(cx).as_local().unwrap().scan_complete())
                .await;
        }
        .boxed_local()
    }

    // This function is similar to flush_fs_events, except that it waits for events to be flushed in
    // the .git folder of the root repository.
    // The reason for its existence is that a repository's .git folder might live *outside* of the
    // worktree and thus its FS events might go through a different path.
    // In order to flush those, we need to create artificial events in the .git folder and wait
    // for the repository to be reloaded.
    #[cfg(feature = "test-support")]
    fn flush_fs_events_in_root_git_repository<'a>(
        &self,
        cx: &'a mut gpui::TestAppContext,
    ) -> futures::future::LocalBoxFuture<'a, ()> {
        let file_name = "fs-event-sentinel";

        let tree = self.clone();
        let (fs, root_path, mut git_dir_scan_id) = self.read_with(cx, |tree, _| {
            let tree = tree.as_local().unwrap();
            let local_repo_entry = tree
                .git_repositories
                .values()
                .min_by_key(|local_repo_entry| local_repo_entry.work_directory.clone())
                .unwrap();
            (
                tree.fs.clone(),
                local_repo_entry.common_dir_abs_path.clone(),
                local_repo_entry.git_dir_scan_id,
            )
        });

        let scan_id_increased = |tree: &mut Worktree, git_dir_scan_id: &mut usize| {
            let tree = tree.as_local().unwrap();
            // let repository = tree.repositories.first().unwrap();
            let local_repo_entry = tree
                .git_repositories
                .values()
                .min_by_key(|local_repo_entry| local_repo_entry.work_directory.clone())
                .unwrap();

            if local_repo_entry.git_dir_scan_id > *git_dir_scan_id {
                *git_dir_scan_id = local_repo_entry.git_dir_scan_id;
                true
            } else {
                false
            }
        };

        async move {
            // Subscribe to events BEFORE creating the file to avoid race condition
            // where events fire before subscription is set up
            let mut events = cx.events(&tree);

            fs.create_file(&root_path.join(file_name), Default::default())
                .await
                .unwrap();

            // Use select to avoid blocking indefinitely if events are delayed
            while !tree.update(cx, |tree, _| scan_id_increased(tree, &mut git_dir_scan_id)) {
                futures::select_biased! {
                    _ = events.next() => {}
                    _ = futures::FutureExt::fuse(cx.background_executor.timer(std::time::Duration::from_millis(10))) => {}
                }
            }

            fs.remove_file(&root_path.join(file_name), Default::default())
                .await
                .unwrap();

            // Use select to avoid blocking indefinitely if events are delayed
            while !tree.update(cx, |tree, _| scan_id_increased(tree, &mut git_dir_scan_id)) {
                futures::select_biased! {
                    _ = events.next() => {}
                    _ = futures::FutureExt::fuse(cx.background_executor.timer(std::time::Duration::from_millis(10))) => {}
                }
            }

            cx.update(|cx| tree.read(cx).as_local().unwrap().scan_complete())
                .await;
        }
        .boxed_local()
    }
}
