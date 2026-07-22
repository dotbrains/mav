use super::*;

impl LocalWorktree {
    pub fn fs(&self) -> &Arc<dyn Fs> {
        &self.fs
    }

    pub fn is_path_private(&self, path: &RelPath) -> bool {
        !self.share_private_files && self.settings.is_path_private(path)
    }

    pub fn fs_is_case_sensitive(&self) -> bool {
        self.fs_case_sensitive
    }

    pub fn scan_complete(&self) -> impl Future<Output = ()> + use<> {
        let mut is_scanning_rx = self.is_scanning.1.clone();
        async move {
            let mut is_scanning = *is_scanning_rx.borrow();
            while is_scanning {
                if let Some(value) = is_scanning_rx.recv().await {
                    is_scanning = value;
                } else {
                    break;
                }
            }
        }
    }

    pub fn wait_for_snapshot(
        &mut self,
        scan_id: usize,
    ) -> impl Future<Output = Result<()>> + use<> {
        let (tx, rx) = oneshot::channel();
        if self.snapshot.completed_scan_id >= scan_id {
            tx.send(()).ok();
        } else {
            match self
                .snapshot_subscriptions
                .binary_search_by_key(&scan_id, |probe| probe.0)
            {
                Ok(ix) | Err(ix) => self.snapshot_subscriptions.insert(ix, (scan_id, tx)),
            }
        }

        async move {
            rx.await?;
            Ok(())
        }
    }

    pub fn snapshot(&self) -> LocalSnapshot {
        self.snapshot.clone()
    }

    pub fn settings(&self) -> WorktreeSettings {
        self.settings.clone()
    }
}
