use super::*;

impl NativeAgent {
    pub(super) fn get_or_create_project_state(
        &mut self,
        project: &Entity<Project>,
        cx: &mut Context<Self>,
    ) -> EntityId {
        let project_id = project.entity_id();
        if self.projects.contains_key(&project_id) {
            return project_id;
        }

        let project_context = cx.new(|_| ProjectContext::new(vec![]));
        self.register_project_with_initial_context(project.clone(), project_context, cx);
        if let Some(state) = self.projects.get_mut(&project_id) {
            state.project_context_needs_refresh.send(()).ok();
        }
        project_id
    }

    fn register_project_with_initial_context(
        &mut self,
        project: Entity<Project>,
        project_context: Entity<ProjectContext>,
        cx: &mut Context<Self>,
    ) {
        let project_id = project.entity_id();

        let context_server_store = project.read(cx).context_server_store();
        let context_server_registry =
            cx.new(|cx| ContextServerRegistry::new(context_server_store.clone(), cx));

        let mut subscriptions = vec![
            cx.subscribe(&project, Self::handle_project_event),
            cx.subscribe(
                &context_server_store,
                Self::handle_context_server_store_updated,
            ),
            cx.subscribe(
                &context_server_registry,
                Self::handle_context_server_registry_event,
            ),
        ];
        // When the user trusts a worktree (or revokes trust), project-local
        // skills become eligible (or ineligible) for loading. Trigger a
        // refresh so the catalog and slash-command list update without a
        // restart. This is unconditional — a `Trusted` event for any
        // worktree under any project is cheap to handle and keeps the
        // logic straightforward.
        if let Some(trusted_worktrees) = TrustedWorktrees::try_get_global(cx) {
            subscriptions.push(
                cx.subscribe(&trusted_worktrees, move |this, _, _event, _cx| {
                    if let Some(state) = this.projects.get_mut(&project_id) {
                        state.project_context_needs_refresh.send(()).ok();
                    }
                }),
            );
        }

        let (project_context_needs_refresh_tx, project_context_needs_refresh_rx) =
            watch::channel(());

        self.projects.insert(
            project_id,
            ProjectState {
                project,
                project_context,
                skills: Arc::new(Vec::new()),
                skill_loading_issues: Vec::new(),
                project_context_needs_refresh: project_context_needs_refresh_tx,
                _maintain_project_context: cx.spawn(async move |this, cx| {
                    Self::maintain_project_context(
                        this,
                        project_id,
                        project_context_needs_refresh_rx,
                        cx,
                    )
                    .await
                }),
                context_server_registry,
                _subscriptions: subscriptions,
            },
        );
    }

    pub(super) fn session_project_state(
        &self,
        session_id: &acp::SessionId,
    ) -> Option<&ProjectState> {
        self.sessions
            .get(session_id)
            .and_then(|session| self.projects.get(&session.project_id))
    }

    async fn maintain_project_context(
        this: WeakEntity<Self>,
        project_id: EntityId,
        mut needs_refresh: watch::Receiver<()>,
        cx: &mut AsyncApp,
    ) -> Result<()> {
        while needs_refresh.changed().await.is_ok() {
            let task = this.update(cx, |this, cx| {
                let state = this
                    .projects
                    .get(&project_id)
                    .context("project state not found")?;
                anyhow::Ok(Self::build_project_context(
                    &state.project,
                    this.fs.clone(),
                    cx,
                ))
            })??;
            let (project_context, skills, skill_issue_data) = task.await;
            let skills = Arc::new(skills);
            let skill_loading_issues: Vec<SkillLoadingIssue> = skill_issue_data
                .into_iter()
                .map(|issue| SkillLoadingIssue {
                    project_id,
                    path: issue.path,
                    message: issue.message.into(),
                    kind: issue.kind,
                })
                .collect();
            this.update(cx, |this, cx| {
                // Only emit SkillLoadingIssuesUpdated when the issue list
                // actually changed. Refreshes happen frequently (prompt-store
                // updates, rules-file edits, worktree events, trust-state
                // changes), and re-emitting an unchanged list causes the UI
                // to redisplay issues the user has already dismissed.
                // Transitions from non-empty to empty still count as a change,
                // so subscribers continue to receive an empty list to clear
                // previously-displayed issues when they get resolved.
                let issues_changed = this
                    .projects
                    .get(&project_id)
                    .map(|state| state.skill_loading_issues != skill_loading_issues)
                    .unwrap_or(true);

                if let Some(state) = this.projects.get_mut(&project_id) {
                    state.skills = skills;
                    state.skill_loading_issues = skill_loading_issues.clone();
                    // Only push the new `ProjectContext` through if it
                    // differs from the current one. The system prompt is
                    // re-rendered from this on every turn, so an unchanged
                    // `ProjectContext` means a byte-identical system prompt
                    // and a continued hit on the model API's prompt cache.
                    // Refreshes fire on many events that don't actually
                    // change what the model sees (e.g. a SKILL.md body edit
                    // that leaves the catalog — name, description, location
                    // — untouched), so this check matters in practice.
                    state
                        .project_context
                        .update(cx, |current_project_context, cx| {
                            if *current_project_context != project_context {
                                *current_project_context = project_context;
                                cx.notify();
                            }
                        });
                }
                if issues_changed {
                    cx.emit(SkillLoadingIssuesUpdated {
                        project_id,
                        issues: skill_loading_issues,
                    });
                }
                // Skills appear in the slash-command list, so a change in
                // the loaded skills needs to be pushed out to active sessions.
                // This runs unconditionally because MCP prompts (also part of
                // the available commands) can change without affecting the
                // skill error list.
                this.update_available_commands_for_project(project_id, cx);
                this.publish_skill_index(cx);
            })?;
        }

        Ok(())
    }

    fn build_project_context(
        project: &Entity<Project>,
        fs: Arc<dyn Fs>,
        cx: &mut App,
    ) -> Task<(ProjectContext, Vec<Skill>, Vec<SkillLoadingIssueData>)> {
        let worktrees = project.read(cx).visible_worktrees(cx).collect::<Vec<_>>();
        let worktree_tasks = worktrees
            .iter()
            .map(|worktree| {
                Self::load_worktree_info_for_system_prompt(worktree.clone(), project.clone(), cx)
            })
            .collect::<Vec<_>>();

        // Load global skills
        let global_skills_task = {
            let global_skills_dir = global_skills_dir();
            let global_skills_fs = fs.clone();
            cx.background_spawn(async move {
                load_skills_from_directory(
                    &global_skills_fs,
                    &global_skills_dir,
                    SkillSource::Global,
                )
                .await
            })
        };

        // Load project-local skills, but only from worktrees the user has
        // trusted. Skills in `.agents/skills/` ship with the project; a
        // freshly cloned untrusted repo can carry hostile descriptions or
        // bodies, so we keep them out of the catalog and the slash-command
        // list until trust is granted. The subscription in
        // `register_project_with_initial_context` triggers a context
        // refresh when a worktree's trust state changes, so newly trusted
        // worktrees pick up their skills without restarting.
        let trusted_worktrees = TrustedWorktrees::try_get_global(cx);
        let worktree_store = project.read(cx).worktree_store();
        let project_skills_task = {
            let project = project.clone();
            let trusted_worktrees = worktrees
                .iter()
                .filter_map(|worktree| {
                    let worktree_id = worktree.read(cx).id();
                    let is_trusted = trusted_worktrees.as_ref().is_none_or(|trusted_worktrees| {
                        trusted_worktrees.update(cx, |trusted_worktrees, cx| {
                            trusted_worktrees.can_trust(&worktree_store, worktree_id, cx)
                        })
                    });
                    if !is_trusted {
                        return None;
                    }

                    let worktree_snapshot = worktree.read(cx);
                    let worktree_root_name: Arc<str> = worktree_snapshot.root_name_str().into();
                    let scan_complete = worktree_snapshot
                        .as_local()
                        .map(|local| local.scan_complete());
                    Some((
                        worktree.clone(),
                        worktree_id,
                        worktree_root_name,
                        scan_complete,
                    ))
                })
                .collect::<Vec<_>>();

            cx.spawn(async move |cx| {
                let mut project_skills_results = Vec::new();
                for (worktree, worktree_id, worktree_root_name, scan_complete) in trusted_worktrees
                {
                    if let Some(scan_complete) = scan_complete {
                        scan_complete.await;
                    }
                    if let Err(error) = expand_project_skills_directories(&worktree, cx).await {
                        project_skills_results.push(vec![Err(SkillLoadError {
                            path: PathBuf::from(project_skills_relative_path()),
                            message: format!("Failed to scan project skills: {}", error),
                        })]);
                        continue;
                    }

                    let skill_files = worktree.update(cx, |worktree, _cx| {
                        project_skill_files_from_worktree(worktree)
                    });
                    let source = SkillSource::ProjectLocal {
                        worktree_id: SkillScopeId(worktree_id.to_usize()),
                        worktree_root_name,
                    };

                    let mut worktree_results = Vec::new();
                    for skill_file in skill_files {
                        if skill_file.size > MAX_SKILL_FILE_SIZE as u64 {
                            worktree_results.push(Err(SkillLoadError {
                                path: skill_file.display_path.clone(),
                                message: format!(
                                    "SKILL.md file exceeds maximum size of {}KB",
                                    MAX_SKILL_FILE_SIZE / 1024
                                ),
                            }));
                            continue;
                        }

                        let buffer = match project
                            .update(cx, |project, cx| {
                                project.open_buffer(
                                    (worktree_id, skill_file.relative_path.clone()),
                                    cx,
                                )
                            })
                            .await
                        {
                            Ok(buffer) => buffer,
                            Err(error) => {
                                worktree_results.push(Err(SkillLoadError {
                                    path: skill_file.display_path.clone(),
                                    message: format!("Failed to read file: {}", error),
                                }));
                                continue;
                            }
                        };

                        let content = cx
                            .update(|cx| buffer.read(cx).as_text_snapshot().as_rope().to_string());

                        worktree_results.push(
                            parse_skill_frontmatter(
                                &skill_file.display_path,
                                &content,
                                source.clone(),
                            )
                            .map_err(|error| SkillLoadError {
                                path: skill_file.display_path,
                                message: error.to_string(),
                            }),
                        );
                    }
                    project_skills_results.push(worktree_results);
                }
                project_skills_results
            })
        };
        cx.spawn(async move |_cx| {
            let worktrees = future::join_all(worktree_tasks).await;

            let worktrees = worktrees
                .into_iter()
                .map(|(worktree, _rules_error)| {
                    // TODO: show error message
                    // if let Some(rules_error) = rules_error {
                    //     this.update(cx, |_, cx| cx.emit(rules_error)).ok();
                    // }
                    worktree
                })
                .collect::<Vec<_>>();

            // Load and combine skills. `combine_skills` deliberately
            // does NOT deduplicate — the autocomplete popup needs to
            // see every entry so users can disambiguate same-named
            // global vs. project-local skills via the source label.
            // Project-overrides-global is applied below, only for the
            // model-facing catalog.
            let global_skills = global_skills_task.await;
            let project_skills_results = project_skills_task.await;
            let (skills, skill_errors) =
                combine_skills(global_skills, project_skills_results.into_iter().flatten());
            let mut skill_issues = skill_errors
                .into_iter()
                .map(SkillLoadingIssueData::from_load_error)
                .collect::<Vec<_>>();
            for skill in &skills {
                skill_issues.extend(
                    skill
                        .load_warnings
                        .iter()
                        .map(|warning| SkillLoadingIssueData::from_load_warning(skill, warning)),
                );
            }

            // Apply project-overrides-global before catalog selection
            // so the model sees at most one entry per name. The full
            // `skills` list is still stored on `ProjectState` and used
            // by the autocomplete popup.
            let overridden = apply_skill_overrides(&skills);

            // Enforce the catalog size budget here so that skills which
            // don't fit produce an issue in the UI rather than being
            // silently swallowed by ProjectContext.
            let (catalog_skills, budget_issues) = select_catalog_skills(&overridden);
            skill_issues.extend(budget_issues);

            let project_context = ProjectContext::new(worktrees).with_skills(catalog_skills);
            (project_context, skills, skill_issues)
        })
    }

    pub(super) fn handle_thread_title_updated(
        &mut self,
        thread: Entity<Thread>,
        _: &TitleUpdated,
        cx: &mut Context<Self>,
    ) {
        let session_id = thread.read(cx).id();
        let Some(session) = self.sessions.get(session_id) else {
            return;
        };

        let thread = thread.downgrade();
        let acp_thread = session.acp_thread.downgrade();
        cx.spawn(async move |_, cx| {
            let title = thread.read_with(cx, |thread, _| thread.title())?;
            if let Some(title) = title {
                let task =
                    acp_thread.update(cx, |acp_thread, cx| acp_thread.set_title(title, cx))?;
                task.await?;
            }
            anyhow::Ok(())
        })
        .detach_and_log_err(cx);
    }

    pub(super) fn handle_thread_token_usage_updated(
        &mut self,
        thread: Entity<Thread>,
        usage: &TokenUsageUpdated,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.sessions.get(thread.read(cx).id()) else {
            return;
        };
        session.acp_thread.update(cx, |acp_thread, cx| {
            acp_thread.update_token_usage(usage.0.clone(), cx);
        });
    }

    fn handle_project_event(
        &mut self,
        project: Entity<Project>,
        event: &project::Event,
        _cx: &mut Context<Self>,
    ) {
        let project_id = project.entity_id();
        let Some(state) = self.projects.get_mut(&project_id) else {
            return;
        };
        match event {
            project::Event::WorktreeAdded(_) | project::Event::WorktreeRemoved(_) => {
                state.project_context_needs_refresh.send(()).ok();
            }
            project::Event::WorktreeUpdatedEntries(_, items) => {
                if items.iter().any(|(path, _, _)| {
                    let path_ref = path.as_ref();
                    RULES_FILE_REL_PATHS
                        .iter()
                        .any(|rules_path| path_ref == rules_path.as_ref())
                        || AGENTS_PREFIX
                            .as_ref()
                            .is_some_and(|prefix| path_ref.starts_with(prefix))
                }) {
                    state.project_context_needs_refresh.send(()).ok();
                }
            }
            _ => {}
        }
    }
}
