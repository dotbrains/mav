use super::*;

impl BackgroundScanner {
    pub(super) async fn progress_timer(&self, running: bool) {
        if !running {
            return futures::future::pending().await;
        }

        #[cfg(feature = "test-support")]
        if self.fs.is_fake() {
            return self.executor.simulate_random_delay().await;
        }

        self.executor.timer(FS_WATCH_LATENCY).await
    }

    pub(super) fn is_path_private(&self, path: &RelPath) -> bool {
        !self.share_private_files && self.settings.is_path_private(path)
    }

    pub(super) fn should_scan_directory(
        &self,
        state: &BackgroundScannerState,
        entry: &Entry,
    ) -> bool {
        let scannable = state.scanning_enabled
            && (!entry.is_external
                || self.settings.scan_symlinks == settings::ScanSymlinksSetting::Always)
            && (!entry.is_ignored || entry.is_always_included);

        scannable
            || entry.path.file_name() == Some(DOT_GIT)
            || entry.path.file_name() == Some(local_settings_folder_name())
            || entry.path.file_name() == Some(local_vscode_folder_name())
            || state.scanned_dirs.contains(&entry.id) // If we've ever scanned it, keep scanning
            || state
                .paths_to_scan
                .iter()
                .any(|p| p.starts_with(&entry.path))
            || state
                .path_prefixes_to_scan
                .iter()
                .any(|p| entry.path.starts_with(p))
    }

    pub(super) async fn next_scan_request(&self) -> Result<ScanRequest> {
        let mut request = self.scan_requests_rx.recv().await?;
        while let Ok(next_request) = self.scan_requests_rx.try_recv() {
            request.relative_paths.extend(next_request.relative_paths);
            request.done.extend(next_request.done);
        }
        Ok(request)
    }
}
