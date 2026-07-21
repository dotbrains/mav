use super::*;

pub(super) fn open_user_settings_in_workspace(
    workspace: &mut Workspace,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let project = workspace.project().clone();

    cx.spawn_in(window, async move |workspace, cx| {
        let (config_dir, settings_file) = project.update(cx, |project, cx| {
            (
                project.try_windows_path_to_wsl(paths::config_dir().as_path(), cx),
                project.try_windows_path_to_wsl(paths::settings_file().as_path(), cx),
            )
        });
        let config_dir = config_dir.await?;
        let settings_file = settings_file.await?;
        project
            .update(cx, |project, cx| {
                project.find_or_create_worktree(&config_dir, false, cx)
            })
            .await
            .ok();
        workspace
            .update_in(cx, |workspace, window, cx| {
                workspace.open_paths(
                    vec![settings_file],
                    OpenOptions {
                        visible: Some(OpenVisible::None),
                        ..Default::default()
                    },
                    None,
                    window,
                    cx,
                )
            })?
            .await;

        workspace.update_in(cx, |_, window, cx| {
            window.activate_window();
            cx.notify();
        })
    })
    .detach();
}

pub(super) fn update_settings_file(
    file: SettingsUiFile,
    file_name: Option<&'static str>,
    window: &mut Window,
    cx: &mut App,
    update: impl 'static + Send + FnOnce(&mut SettingsContent, &App),
) -> Result<()> {
    telemetry::event!("Settings Change", setting = file_name, type = file.setting_type());

    match file {
        SettingsUiFile::Project((worktree_id, rel_path)) => {
            let rel_path = rel_path.join(paths::local_settings_file_relative_path());
            let Some(settings_window) = window.root::<SettingsWindow>().flatten() else {
                anyhow::bail!("No settings window found");
            };

            update_project_setting_file(worktree_id, rel_path, update, settings_window, cx)
        }
        SettingsUiFile::User => {
            // todo(settings_ui) error?
            SettingsStore::global(cx).update_settings_file(<dyn fs::Fs>::global(cx), update);
            Ok(())
        }
        SettingsUiFile::Server(_) => unimplemented!(),
    }
}

pub(super) struct ProjectSettingsUpdateEntry {
    worktree_id: WorktreeId,
    rel_path: Arc<RelPath>,
    settings_window: WeakEntity<SettingsWindow>,
    project: WeakEntity<Project>,
    worktree: WeakEntity<Worktree>,
    update: Box<dyn FnOnce(&mut SettingsContent, &App)>,
}

pub(super) struct ProjectSettingsUpdateQueue {
    tx: mpsc::UnboundedSender<ProjectSettingsUpdateEntry>,
    _task: Task<()>,
}

impl Global for ProjectSettingsUpdateQueue {}

impl ProjectSettingsUpdateQueue {
    pub(super) fn new(cx: &mut App) -> Self {
        let (tx, mut rx) = mpsc::unbounded();
        let task = cx.spawn(async move |mut cx| {
            while let Some(entry) = rx.next().await {
                if let Err(err) = Self::process_entry(entry, &mut cx).await {
                    log::error!("Failed to update project settings: {err:?}");
                }
            }
        });
        Self { tx, _task: task }
    }

    pub(super) fn enqueue(cx: &mut App, entry: ProjectSettingsUpdateEntry) {
        cx.update_global::<Self, _>(|queue, _cx| {
            if let Err(err) = queue.tx.unbounded_send(entry) {
                log::error!("Failed to enqueue project settings update: {err}");
            }
        });
    }

    async fn process_entry(entry: ProjectSettingsUpdateEntry, cx: &mut AsyncApp) -> Result<()> {
        let ProjectSettingsUpdateEntry {
            worktree_id,
            rel_path,
            settings_window,
            project,
            worktree,
            update,
        } = entry;

        let project_path = ProjectPath {
            worktree_id,
            path: rel_path.clone(),
        };

        let needs_creation = worktree.read_with(cx, |worktree, _| {
            worktree.entry_for_path(&rel_path).is_none()
        })?;

        if needs_creation {
            worktree
                .update(cx, |worktree, cx| {
                    worktree.create_entry(rel_path.clone(), false, None, cx)
                })?
                .await?;
        }

        let buffer_store = project.read_with(cx, |project, _cx| project.buffer_store().clone())?;

        let cached_buffer = settings_window
            .read_with(cx, |settings_window, _| {
                settings_window
                    .project_setting_file_buffers
                    .get(&project_path)
                    .cloned()
            })
            .unwrap_or_default();

        let buffer = if let Some(cached_buffer) = cached_buffer {
            let needs_reload = cached_buffer.read_with(cx, |buffer, _| buffer.has_conflict());
            if needs_reload {
                cached_buffer
                    .update(cx, |buffer, cx| buffer.reload(cx))
                    .await
                    .context("Failed to reload settings file")?;
            }
            cached_buffer
        } else {
            let buffer = buffer_store
                .update(cx, |store, cx| store.open_buffer(project_path.clone(), cx))
                .await
                .context("Failed to open settings file")?;

            let _ = settings_window.update(cx, |this, _cx| {
                this.project_setting_file_buffers
                    .insert(project_path, buffer.clone());
            });

            buffer
        };

        buffer.update(cx, |buffer, cx| {
            let current_text = buffer.text();
            if let Some(new_text) = cx
                .global::<SettingsStore>()
                .new_text_for_update(current_text, |settings| update(settings, cx))
                .log_err()
            {
                buffer.edit([(0..buffer.len(), new_text)], None, cx);
            }
        });

        buffer_store
            .update(cx, |store, cx| store.save_buffer(buffer, cx))
            .await
            .context("Failed to save settings file")?;

        Ok(())
    }
}

fn update_project_setting_file(
    worktree_id: WorktreeId,
    rel_path: Arc<RelPath>,
    update: impl 'static + FnOnce(&mut SettingsContent, &App),
    settings_window: Entity<SettingsWindow>,
    cx: &mut App,
) -> Result<()> {
    let Some((worktree, project)) =
        all_projects(settings_window.read(cx).original_window.as_ref(), cx).find_map(|project| {
            project
                .read(cx)
                .worktree_for_id(worktree_id, cx)
                .zip(Some(project))
        })
    else {
        anyhow::bail!("Could not find project with worktree id: {}", worktree_id);
    };

    let entry = ProjectSettingsUpdateEntry {
        worktree_id,
        rel_path,
        settings_window: settings_window.downgrade(),
        project: project.downgrade(),
        worktree: worktree.downgrade(),
        update: Box::new(update),
    };

    ProjectSettingsUpdateQueue::enqueue(cx, entry);

    Ok(())
}
