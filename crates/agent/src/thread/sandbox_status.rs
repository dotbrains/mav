use super::*;

impl Thread {
    /// The sandbox grants configured for this thread, using unverified Git path
    /// candidates. Use [`Self::refresh_verified_sandbox_status`] for UI or other
    /// surfaces that need to match terminal enforcement.
    pub fn sandbox_status(&self, cx: &App) -> Option<(ThreadSandbox, ThreadSandbox)> {
        if !self.sandboxing_available(cx) {
            return None;
        }
        let persistent = AgentSettings::get_global(cx).sandbox_permissions.clone();
        let git_dirs = crate::sandboxing::sandbox_git_dirs(self.project.read(cx), cx);
        let grants = self.sandbox_grants.borrow();
        let settings = crate::sandboxing::settings_thread_sandbox(&persistent)
            .with_git(persistent.allow_git_access, git_dirs.clone());
        let thread = grants
            .thread_sandbox()
            .with_git(grants.git_access_granted(), git_dirs);
        Some((settings, thread))
    }

    pub fn refresh_verified_sandbox_status(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<(SandboxStatusKey, SandboxStatusRefresh)> {
        if !self.sandboxing_available(cx) {
            return None;
        }

        let persistent = AgentSettings::get_global(cx).sandbox_permissions.clone();
        let settings_sandbox = crate::sandboxing::settings_thread_sandbox(&persistent);
        let grants = self.sandbox_grants.borrow();
        let thread_sandbox = grants.thread_sandbox();
        let thread_allow_git_access = grants.git_access_granted();
        drop(grants);

        let (sandbox_path_candidates, fs) = {
            let project = self.project.read(cx);
            (
                SandboxGitPathCandidates::from_project(project, cx),
                project.fs().clone(),
            )
        };
        let baseline_writable_paths = sandbox_path_candidates.writable_paths.clone();
        let git_paths = sandbox_path_candidates.git_paths.clone();
        let repository_paths = sandbox_path_candidates.cache_key_repositories();

        let key = SandboxStatusKey {
            settings_sandbox: settings_sandbox.clone(),
            thread_sandbox: thread_sandbox.clone(),
            baseline_writable_paths: baseline_writable_paths.clone(),
            git_paths: git_paths.clone(),
            repository_paths,
            settings_allow_git_access: persistent.allow_git_access,
            thread_allow_git_access,
        };

        if settings_sandbox.is_unsandboxed() || thread_sandbox.is_unsandboxed() {
            return Some((
                key,
                SandboxStatusRefresh::Ready(VerifiedSandboxStatus {
                    settings_sandbox,
                    thread_sandbox,
                    baseline_writable_paths,
                }),
            ));
        }

        let git_access_requested = persistent.allow_git_access || thread_allow_git_access;
        if !git_access_requested {
            return Some((
                key,
                SandboxStatusRefresh::Ready(VerifiedSandboxStatus {
                    settings_sandbox: settings_sandbox.with_git(false, git_paths.clone()),
                    thread_sandbox: thread_sandbox.with_git(false, git_paths),
                    baseline_writable_paths,
                }),
            ));
        }

        let task = cx.spawn(async move |_this, _cx| {
            let sandbox_paths = sandbox_git_paths(sandbox_path_candidates, fs.as_ref(), true).await;
            VerifiedSandboxStatus {
                settings_sandbox: settings_sandbox.with_git(
                    persistent.allow_git_access && sandbox_paths.allow_git_access,
                    sandbox_paths.git_dirs.clone(),
                ),
                thread_sandbox: thread_sandbox.with_git(
                    thread_allow_git_access && sandbox_paths.allow_git_access,
                    sandbox_paths.git_dirs,
                ),
                baseline_writable_paths,
            }
        });

        Some((key, SandboxStatusRefresh::Pending(task)))
    }

    /// Whether agent terminal commands are sandboxed for this thread's project,
    /// so the UI can decide whether to surface the sandbox status at all.
    pub fn sandboxing_enabled(&self, cx: &App) -> bool {
        sandboxing_enabled_for_project(self.project.read(cx), cx)
    }

    /// Whether sandboxing is *applicable* for this thread's project (feature on,
    /// local project, supported platform), regardless of whether it's been
    /// turned off in settings. The UI shows the sandbox indicator whenever this
    /// is true, drawing it struck-out when sandboxing is disabled.
    pub fn sandboxing_available(&self, cx: &App) -> bool {
        sandboxing_available_for_project(self.project.read(cx), cx)
    }

    /// The directory subtrees the sandbox always grants write access to for this
    /// thread's project (its worktree roots), derived from the same source the
    /// terminal tool uses when it actually builds the sandbox.
    pub fn sandbox_baseline_writable_paths(&self, cx: &App) -> Vec<PathBuf> {
        crate::sandboxing::sandbox_worktree_writable_paths(self.project.read(cx), cx)
    }
}
