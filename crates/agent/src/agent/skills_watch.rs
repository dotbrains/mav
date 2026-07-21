use super::*;

#[derive(Default)]
pub(super) enum SkillsState {
    /// No scan or watch is active. A user-interaction trigger will kick
    /// off a fresh scan.
    #[default]
    Idle,
    /// A one-shot scan task is in flight. It checks whether
    /// `~/.agents/skills/` exists; if so, transitions to `Watching`,
    /// otherwise back to `Idle`.
    Scanning,
    /// A watch task is observing `~/.agents/skills/`. It transitions
    /// back to `Idle` if the watched directory itself is removed.
    Watching,
}

impl NativeAgent {
    /// Kicks off a one-time scan of the global skills directory if one
    /// isn't already in progress and a watch isn't already active.
    ///
    /// Idempotent and cheap: returns immediately if a scan or watch is
    /// already running. The expected callers are user-interaction events
    /// from the agent panel (input focus, slash autocomplete, conversation
    /// submit); firing this from any of them is equivalent and safe to
    /// repeat.
    ///
    /// The scan itself runs detached on the foreground executor. If
    /// `~/.agents/skills/` exists it transitions state to
    /// [`SkillsState::Watching`] and starts a recursive watch;
    /// otherwise it transitions back to [`SkillsState::Idle`] so the
    /// next trigger retries (covering the case where the user creates
    /// the directory after the first scan).
    pub fn ensure_skills_scan_started(&mut self, cx: &mut Context<Self>) {
        if !matches!(self.skills_state, SkillsState::Idle) {
            return;
        }
        self.skills_state = SkillsState::Scanning;
        let fs = self.fs.clone();
        cx.spawn(async move |this, cx| Self::run_skills_scan(this, fs, cx).await)
            .detach();
    }

    async fn run_skills_scan(this: WeakEntity<Self>, fs: Arc<dyn Fs>, cx: &mut AsyncApp) {
        let skills_dir = global_skills_dir();
        if !fs.is_dir(&skills_dir).await {
            // Skills directory doesn't exist; revert state so the next
            // user trigger retries.
            let _ = this.update(cx, |this, _cx| {
                this.skills_state = SkillsState::Idle;
            });
            return;
        }

        // Skills directory exists. Start a watch and trigger a refresh
        // of every project's context so the freshly-discovered skills
        // get loaded.
        let _ = this.update(cx, |this, cx| {
            cx.spawn({
                let fs = fs.clone();
                let skills_dir = skills_dir.clone();
                async move |this, cx| Self::run_skills_watch(this, fs, skills_dir, cx).await
            })
            .detach();
            this.skills_state = SkillsState::Watching;
            for state in this.projects.values_mut() {
                state.project_context_needs_refresh.send(()).ok();
            }
        });
    }

    async fn run_skills_watch(
        this: WeakEntity<Self>,
        fs: Arc<dyn Fs>,
        skills_dir: PathBuf,
        cx: &mut AsyncApp,
    ) {
        let (mut events, watcher) = fs
            .watch(&skills_dir, std::time::Duration::from_millis(500))
            .await;

        // Linux's inotify backend is non-recursive, so a watch on
        // `skills_dir` only fires for direct children. Skill discovery
        // is intentionally one level deep (`<skills_dir>/<skill>/SKILL.md`),
        // so we only register watches on each immediate child directory
        // and deliberately do NOT recurse: a stray `node_modules`,
        // `target`, or `.git` inside a skill folder would otherwise
        // register watches for tens of thousands of subdirectories.
        // These per-child adds are cheap no-ops on macOS/Windows where
        // the OS-level watch is already recursive.
        if let Ok(mut entries) = fs.read_dir(&skills_dir).await {
            while let Some(entry) = entries.next().await {
                let Ok(path) = entry else { continue };
                if let Ok(Some(metadata)) = fs.metadata(&path).await
                    && metadata.is_dir
                {
                    watcher.add(&path).ok();
                }
            }
        }

        while let Some(events) = events.next().await {
            // When a new immediate child directory of `skills_dir` is
            // created, add a single watch for it so changes to its
            // `SKILL.md` are observed on Linux. We intentionally do not
            // recurse into the new directory — skill discovery is only
            // one level deep.
            for event in &events {
                if event.kind == Some(fs::PathEventKind::Created)
                    && event.path.parent() == Some(skills_dir.as_path())
                    && fs.is_dir(&event.path).await
                {
                    watcher.add(&event.path).ok();
                }
            }

            let watched_root_removed = events.iter().any(|event| {
                event.path == skills_dir && event.kind == Some(fs::PathEventKind::Removed)
            });

            let updated = this.update(cx, |this, _cx| {
                for state in this.projects.values_mut() {
                    state.project_context_needs_refresh.send(()).ok();
                }
                if watched_root_removed {
                    // Drop back to Idle so the next user trigger
                    // retries the scan; the next trigger will rediscover
                    // the directory if the user has recreated it.
                    this.skills_state = SkillsState::Idle;
                }
            });
            if updated.is_err() || watched_root_removed {
                return;
            }
        }
    }
}
