use super::*;

impl BackgroundScanner {
    pub(super) async fn process_scan_request(
        &self,
        mut request: ScanRequest,
        scanning: bool,
    ) -> bool {
        log::debug!("rescanning paths {:?}", request.relative_paths);

        request.relative_paths.sort_unstable();
        self.forcibly_load_paths(&request.relative_paths).await;

        let root_path = self.state.lock().await.snapshot.abs_path.clone();
        let root_canonical_path = self.fs.canonicalize(root_path.as_path()).await;
        let root_canonical_path = match &root_canonical_path {
            Ok(path) => SanitizedPath::new(path),
            Err(err) => {
                log::error!("failed to canonicalize root path {root_path:?}: {err:#}");
                return true;
            }
        };
        let abs_paths = request
            .relative_paths
            .iter()
            .map(|path| {
                if path.file_name().is_some() {
                    root_canonical_path.as_path().join(path.as_std_path())
                } else {
                    root_canonical_path.as_path().to_path_buf()
                }
            })
            .collect::<Vec<_>>();

        {
            let mut state = self.state.lock().await;
            let is_idle = state.snapshot.completed_scan_id == state.snapshot.scan_id;
            state.snapshot.scan_id += 1;
            if is_idle {
                state.snapshot.completed_scan_id = state.snapshot.scan_id;
            }
        }

        self.reload_entries_for_paths(
            &root_path,
            &root_canonical_path,
            &request.relative_paths,
            abs_paths,
            None,
        )
        .await;

        self.send_status_update(scanning, request.done, &[]).await
    }
}
