use super::*;

impl BackgroundScanner {
    pub(super) async fn update_global_gitignore(&self, abs_path: &Path) {
        let ignore = build_gitignore(abs_path, self.fs.as_ref())
            .await
            .log_err()
            .map(Arc::new);
        let (prev_snapshot, ignore_stack, abs_path) = {
            let mut state = self.state.lock().await;
            state.snapshot.global_gitignore = ignore;
            let abs_path = state.snapshot.abs_path().clone();
            let ignore_stack = state
                .snapshot
                .ignore_stack_for_abs_path(&abs_path, true, self.fs.as_ref())
                .await;
            (state.snapshot.clone(), ignore_stack, abs_path)
        };
        let (scan_job_tx, scan_job_rx) = async_channel::unbounded();
        self.update_ignore_statuses_for_paths(
            scan_job_tx,
            prev_snapshot,
            vec![(abs_path, ignore_stack)],
        )
        .await;
        self.scan_dirs(false, scan_job_rx).await;
        self.send_status_update(false, SmallVec::new(), &[]).await;
    }
}
