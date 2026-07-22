use super::*;

impl Worktree {
    pub fn as_local(&self) -> Option<&LocalWorktree> {
        if let Worktree::Local(worktree) = self {
            Some(worktree)
        } else {
            None
        }
    }

    pub fn as_remote(&self) -> Option<&RemoteWorktree> {
        if let Worktree::Remote(worktree) = self {
            Some(worktree)
        } else {
            None
        }
    }

    pub fn as_local_mut(&mut self) -> Option<&mut LocalWorktree> {
        if let Worktree::Local(worktree) = self {
            Some(worktree)
        } else {
            None
        }
    }

    pub fn as_remote_mut(&mut self) -> Option<&mut RemoteWorktree> {
        if let Worktree::Remote(worktree) = self {
            Some(worktree)
        } else {
            None
        }
    }

    pub fn is_local(&self) -> bool {
        matches!(self, Worktree::Local(_))
    }

    pub fn is_remote(&self) -> bool {
        !self.is_local()
    }

    pub fn settings_location(&self, _: &Context<Self>) -> SettingsLocation<'static> {
        SettingsLocation {
            worktree_id: self.id(),
            path: RelPath::empty(),
        }
    }

    pub fn snapshot(&self) -> Snapshot {
        match self {
            Worktree::Local(worktree) => worktree.snapshot.snapshot.clone(),
            Worktree::Remote(worktree) => worktree.snapshot.clone(),
        }
    }

    pub fn scan_id(&self) -> usize {
        match self {
            Worktree::Local(worktree) => worktree.snapshot.scan_id,
            Worktree::Remote(worktree) => worktree.snapshot.scan_id,
        }
    }

    pub fn metadata_proto(&self) -> proto::WorktreeMetadata {
        proto::WorktreeMetadata {
            id: self.id().to_proto(),
            root_name: self.root_name().to_proto(),
            visible: self.is_visible(),
            abs_path: self.abs_path().to_string_lossy().into_owned(),
            root_repo_common_dir: self
                .root_repo_common_dir()
                .map(|p| p.to_string_lossy().into_owned()),
        }
    }

    pub fn completed_scan_id(&self) -> usize {
        match self {
            Worktree::Local(worktree) => worktree.snapshot.completed_scan_id,
            Worktree::Remote(worktree) => worktree.snapshot.completed_scan_id,
        }
    }

    pub fn is_visible(&self) -> bool {
        match self {
            Worktree::Local(worktree) => worktree.visible,
            Worktree::Remote(worktree) => worktree.visible,
        }
    }

    pub fn replica_id(&self) -> ReplicaId {
        match self {
            Worktree::Local(_) => ReplicaId::LOCAL,
            Worktree::Remote(worktree) => worktree.replica_id,
        }
    }

    pub fn abs_path(&self) -> Arc<Path> {
        match self {
            Worktree::Local(worktree) => SanitizedPath::cast_arc(worktree.abs_path.clone()),
            Worktree::Remote(worktree) => SanitizedPath::cast_arc(worktree.abs_path.clone()),
        }
    }

    pub fn root_file(&self, cx: &Context<Self>) -> Option<Arc<File>> {
        let entry = self.root_entry()?;
        Some(File::for_entry(entry.clone(), cx.entity()))
    }

    pub fn observe_updates<F, Fut>(&mut self, project_id: u64, cx: &Context<Worktree>, callback: F)
    where
        F: 'static + Send + Fn(proto::UpdateWorktree) -> Fut,
        Fut: 'static + Send + Future<Output = bool>,
    {
        match self {
            Worktree::Local(this) => this.observe_updates(project_id, cx, callback),
            Worktree::Remote(this) => this.observe_updates(project_id, cx, callback),
        }
    }

    pub fn stop_observing_updates(&mut self) {
        match self {
            Worktree::Local(this) => {
                this.update_observer.take();
            }
            Worktree::Remote(this) => {
                this.update_observer.take();
            }
        }
    }

    pub fn wait_for_snapshot(
        &mut self,
        scan_id: usize,
    ) -> impl Future<Output = Result<()>> + use<> {
        match self {
            Worktree::Local(this) => this.wait_for_snapshot(scan_id).boxed(),
            Worktree::Remote(this) => this.wait_for_snapshot(scan_id).boxed(),
        }
    }

    #[cfg(feature = "test-support")]
    pub fn has_update_observer(&self) -> bool {
        match self {
            Worktree::Local(this) => this.update_observer.is_some(),
            Worktree::Remote(this) => this.update_observer.is_some(),
        }
    }
}
