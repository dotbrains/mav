use super::*;

impl GitPanel {
    pub fn load_commit_template(
        &self,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<Option<GitCommitTemplate>>> {
        let Some(repo) = self.active_repository.clone() else {
            return Task::ready(Err(anyhow::anyhow!("no active repo")));
        };
        repo.update(cx, |repo, cx| {
            let rx = repo.load_commit_template_text();
            cx.spawn(async move |_, _| rx.await?)
        })
    }

    pub fn amend_pending(&self) -> bool {
        self.amend_pending
    }

    /// Sets the pending amend state, ensuring that the original commit message
    /// is either saved, when `value` is `true` and there's no pending amend, or
    /// restored, when `value` is `false` and there's a pending amend.
    pub fn set_amend_pending(&mut self, value: bool, cx: &mut Context<Self>) {
        if value && !self.amend_pending {
            let current_message = self.commit_message_buffer(cx).read(cx).text();
            self.original_commit_message = if current_message.trim().is_empty() {
                None
            } else {
                Some(current_message)
            };
        } else if !value && self.amend_pending {
            let message = self.original_commit_message.take().unwrap_or_default();
            self.commit_message_buffer(cx).update(cx, |buffer, cx| {
                let start = buffer.anchor_before(0);
                let end = buffer.anchor_after(buffer.len());
                buffer.edit([(start..end, message)], None, cx);
            });
        }

        self.amend_pending = value;
        self.serialize(cx);
        cx.notify();
    }

    pub fn signoff_enabled(&self) -> bool {
        self.signoff_enabled
    }

    pub fn set_signoff_enabled(&mut self, value: bool, cx: &mut Context<Self>) {
        self.signoff_enabled = value;
        self.serialize(cx);
        cx.notify();
    }

    pub fn toggle_signoff_enabled(
        &mut self,
        _: &Signoff,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_signoff_enabled(!self.signoff_enabled, cx);
    }

    pub async fn load(
        workspace: WeakEntity<Workspace>,
        mut cx: AsyncWindowContext,
    ) -> anyhow::Result<Entity<Self>> {
        let serialized_panel = match workspace
            .read_with(&cx, |workspace, cx| {
                Self::serialization_key(workspace).map(|key| (key, KeyValueStore::global(cx)))
            })
            .ok()
            .flatten()
        {
            Some((serialization_key, kvp)) => cx
                .background_spawn(async move { kvp.read_kvp(&serialization_key) })
                .await
                .context("loading git panel")
                .log_err()
                .flatten()
                .map(|panel| serde_json::from_str::<SerializedGitPanel>(&panel))
                .transpose()
                .log_err()
                .flatten(),
            None => None,
        };

        workspace.update_in(&mut cx, |workspace, window, cx| {
            GitPanel::new_with_serialized_panel(workspace, serialized_panel, window, cx)
        })
    }

    pub(super) fn stage_bulk(&mut self, mut index: usize, cx: &mut Context<'_, Self>) {
        let Some(op) = self.bulk_staging.as_ref() else {
            return;
        };
        let Some(mut anchor_index) = self.entry_by_path(&op.anchor) else {
            return;
        };
        if let Some(entry) = self.entries.get(index)
            && let Some(entry) = entry.status_entry()
        {
            self.set_bulk_staging_anchor(entry.repo_path.clone(), cx);
        }
        if index < anchor_index {
            std::mem::swap(&mut index, &mut anchor_index);
        }
        let entries = self
            .entries
            .get(anchor_index..=index)
            .unwrap_or_default()
            .iter()
            .filter_map(|entry| entry.status_entry().cloned())
            .collect::<Vec<_>>();
        self.change_file_stage(true, entries, cx);
    }

    pub(super) fn set_bulk_staging_anchor(
        &mut self,
        path: RepoPath,
        cx: &mut Context<'_, GitPanel>,
    ) {
        let Some(repo) = self.active_repository.as_ref() else {
            return;
        };
        self.bulk_staging = Some(BulkStaging {
            repo_id: repo.read(cx).id,
            anchor: path,
        });
    }

    pub(crate) fn toggle_amend_pending(&mut self, cx: &mut Context<Self>) {
        self.set_amend_pending(!self.amend_pending, cx);
        if self.amend_pending {
            self.load_last_commit_message(cx);
        }
    }
}
