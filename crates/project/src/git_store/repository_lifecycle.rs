use super::*;

impl Repository {
    pub fn is_trusted(&self) -> bool {
        match self.repository_state.peek() {
            Some(Ok(RepositoryState::Local(state))) => state.backend.is_trusted(),
            _ => false,
        }
    }

    pub fn snapshot(&self) -> RepositorySnapshot {
        self.snapshot.clone()
    }

    pub fn pending_ops(&self) -> impl Iterator<Item = PendingOps> + '_ {
        self.pending_ops.iter().cloned()
    }

    pub fn pending_ops_summary(&self) -> PathSummary<PendingOpsSummary> {
        self.pending_ops.summary().clone()
    }

    pub fn pending_ops_for_path(&self, path: &RepoPath) -> Option<PendingOps> {
        self.pending_ops
            .get(&PathKey(path.as_ref().clone()), ())
            .cloned()
    }

    pub(super) fn respawn_local_worker(
        &mut self,
        project_environment: WeakEntity<ProjectEnvironment>,
        fs: Arc<dyn Fs>,
        is_trusted: bool,
        cx: &mut Context<Self>,
    ) {
        let work_directory_abs_path = self.snapshot.work_directory_abs_path.clone();
        let dot_git_abs_path = self.snapshot.dot_git_abs_path.clone();

        let state = cx
            .spawn(async move |_, cx| {
                LocalRepositoryState::new(
                    work_directory_abs_path,
                    dot_git_abs_path,
                    project_environment,
                    fs,
                    is_trusted,
                    cx,
                )
                .await
                .map_err(|err| err.to_string())
            })
            .shared();
        self.job_sender.close_channel();
        self._worker_task = Task::ready(());
        self.active_jobs.clear();
        self.job_debug_queue
            .mark_unfinished_complete(job_debug_queue::CompletedJobStatus::Skipped);
        cx.notify();

        let (job_sender, worker_task) = Repository::spawn_local_git_worker(state.clone(), cx);
        self.job_sender = job_sender;
        self._worker_task = worker_task;
        self.repository_state = cx
            .spawn(async move |_, _| {
                let state = state.await?;
                Ok(RepositoryState::Local(state))
            })
            .shared();
    }

    pub(super) fn reinitialize_local_backend(
        &mut self,
        work_directory_abs_path: Arc<Path>,
        dot_git_abs_path: Arc<Path>,
        repository_dir_abs_path: Arc<Path>,
        common_dir_abs_path: Arc<Path>,
        project_environment: WeakEntity<ProjectEnvironment>,
        fs: Arc<dyn Fs>,
        is_trusted: bool,
        cx: &mut Context<Self>,
    ) {
        self.snapshot.work_directory_abs_path = work_directory_abs_path;
        self.snapshot.dot_git_abs_path = dot_git_abs_path;
        self.snapshot.repository_dir_abs_path = repository_dir_abs_path;
        self.snapshot.common_dir_abs_path = common_dir_abs_path;
        self.respawn_local_worker(project_environment, fs, is_trusted, cx);
    }

    pub(super) fn local(
        id: RepositoryId,
        work_directory_abs_path: Arc<Path>,
        repository_dir_abs_path: Arc<Path>,
        common_dir_abs_path: Arc<Path>,
        dot_git_abs_path: Arc<Path>,
        project_environment: WeakEntity<ProjectEnvironment>,
        fs: Arc<dyn Fs>,
        is_trusted: bool,
        git_store: WeakEntity<GitStore>,
        cx: &mut Context<Self>,
    ) -> Self {
        let snapshot = RepositorySnapshot::empty(
            id,
            work_directory_abs_path,
            Some(repository_dir_abs_path),
            Some(dot_git_abs_path),
            Some(common_dir_abs_path),
            PathStyle::local(),
        );

        let mut repo = Repository {
            this: cx.weak_entity(),
            git_store,
            snapshot,
            pending_ops: Default::default(),
            repository_state: Task::ready(Err("not yet initialized".into())).shared(),
            _worker_task: Task::ready(()),
            commit_message_buffer: None,
            askpass_delegates: Default::default(),
            paths_needing_status_update: Default::default(),
            latest_askpass_id: 0,
            job_sender: mpsc::unbounded().0,
            job_id: 0,
            active_jobs: Default::default(),
            job_debug_queue: job_debug_queue::GitJobDebugQueue::new(),
            initial_graph_data: Default::default(),
            commit_data: Default::default(),
            commit_data_handler: CommitDataHandlerState::Closed,
        };
        repo.respawn_local_worker(project_environment, fs, is_trusted, cx);
        cx.subscribe_self(Self::handle_subscribe_self).detach();
        repo
    }

    pub(super) fn remote(
        id: RepositoryId,
        work_directory_abs_path: Arc<Path>,
        repository_dir_abs_path: Option<Arc<Path>>,
        common_dir_abs_path: Option<Arc<Path>>,
        path_style: PathStyle,
        project_id: ProjectId,
        client: AnyProtoClient,
        git_store: WeakEntity<GitStore>,
        cx: &mut Context<Self>,
    ) -> Self {
        let snapshot = RepositorySnapshot::empty(
            id,
            work_directory_abs_path,
            repository_dir_abs_path,
            None,
            common_dir_abs_path,
            path_style,
        );

        let repository_state = RemoteRepositoryState { project_id, client };
        let (job_sender, worker_task) = Self::spawn_remote_git_worker(repository_state.clone(), cx);
        let repository_state = Task::ready(Ok(RepositoryState::Remote(repository_state))).shared();
        cx.subscribe_self(Self::handle_subscribe_self).detach();

        Self {
            this: cx.weak_entity(),
            snapshot,
            commit_message_buffer: None,
            git_store,
            pending_ops: Default::default(),
            paths_needing_status_update: Default::default(),
            job_sender,
            _worker_task: worker_task,
            repository_state,
            askpass_delegates: Default::default(),
            latest_askpass_id: 0,
            active_jobs: Default::default(),
            job_debug_queue: job_debug_queue::GitJobDebugQueue::new(),
            job_id: 0,
            initial_graph_data: Default::default(),
            commit_data: Default::default(),
            commit_data_handler: CommitDataHandlerState::Closed,
        }
    }

    fn handle_subscribe_self(&mut self, event: &RepositoryEvent, _: &mut Context<Self>) {
        // scan id greater than 2 means the initial snapshot was calculated,
        // otherwise we don't need to refresh the graph state
        match event {
            RepositoryEvent::HeadChanged | RepositoryEvent::BranchListChanged => {
                if self.scan_id > 2 {
                    self.initial_graph_data.clear();
                }
            }
            RepositoryEvent::StashEntriesChanged => {
                if self.scan_id > 2 {
                    self.initial_graph_data
                        .retain(|(log_source, _), _| *log_source != LogSource::All);
                }
            }
            _ => {}
        }
    }

    pub fn git_store(&self) -> Option<Entity<GitStore>> {
        self.git_store.upgrade()
    }
}
