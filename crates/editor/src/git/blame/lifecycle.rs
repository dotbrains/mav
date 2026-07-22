use super::*;

impl GitBlame {
    pub fn new(
        multi_buffer: Entity<MultiBuffer>,
        project: Entity<Project>,
        user_triggered: bool,
        focused: bool,
        cx: &mut Context<Self>,
    ) -> Self {
        let multi_buffer_subscription = cx.subscribe(
            &multi_buffer,
            |git_blame, multi_buffer, event, cx| match event {
                multi_buffer::Event::DirtyChanged => {
                    if !multi_buffer.read(cx).is_dirty(cx) {
                        git_blame.generate(cx);
                    }
                }
                multi_buffer::Event::BufferRangesUpdated { .. }
                | multi_buffer::Event::BuffersEdited { .. } => git_blame.regenerate_on_edit(cx),
                _ => {}
            },
        );
        let project_subscription = cx.subscribe(&project, {
            let multi_buffer = multi_buffer.downgrade();

            move |git_blame, _, event, cx| {
                if let project::Event::WorktreeUpdatedEntries(_, updated) = event {
                    let Some(multi_buffer) = multi_buffer.upgrade() else {
                        return;
                    };
                    let project_entry_id = multi_buffer
                        .read(cx)
                        .as_singleton()
                        .and_then(|it| it.read(cx).entry_id(cx));
                    if updated
                        .iter()
                        .any(|(_, entry_id, _)| project_entry_id == Some(*entry_id))
                    {
                        log::debug!("Updated buffers. Regenerating blame data...",);
                        git_blame.generate(cx);
                    }
                }
            }
        });

        let git_store = project.read(cx).git_store().clone();
        let git_store_subscription =
            cx.subscribe(&git_store, move |this, _, event, cx| match event {
                GitStoreEvent::RepositoryUpdated(_, _, _)
                | GitStoreEvent::RepositoryAdded
                | GitStoreEvent::RepositoryRemoved(_) => {
                    log::debug!("Status of git repositories updated. Regenerating blame data...",);
                    this.generate(cx);
                }
                _ => {}
            });

        let mut this = Self {
            project,
            multi_buffer: multi_buffer.downgrade(),
            buffers: HashMap::default(),
            user_triggered,
            focused,
            changed_while_blurred: false,
            task: Task::ready(Ok(())),
            regenerate_on_edit_task: Task::ready(Ok(())),
            _regenerate_subscriptions: vec![
                multi_buffer_subscription,
                project_subscription,
                git_store_subscription,
            ],
        };
        this.generate(cx);
        this
    }

    pub fn repository(&self, cx: &App, id: BufferId) -> Option<Entity<Repository>> {
        self.project
            .read(cx)
            .git_store()
            .read(cx)
            .repository_and_path_for_buffer_id(id, cx)
            .map(|(repo, _)| repo)
    }

    pub fn has_generated_entries(&self) -> bool {
        !self.buffers.is_empty()
    }

    pub fn details_for_entry(
        &self,
        buffer: BufferId,
        entry: &BlameEntry,
    ) -> Option<ParsedCommitMessage> {
        self.buffers
            .get(&buffer)?
            .commit_details
            .get(&entry.sha)
            .cloned()
    }

    pub fn blame_for_rows<'a>(
        &'a mut self,
        rows: &'a [RowInfo],
        cx: &'a mut App,
    ) -> impl Iterator<Item = Option<(BufferId, BlameEntry)>> + use<'a> {
        rows.iter().map(move |info| {
            let buffer_id = info.buffer_id?;
            self.sync(cx, buffer_id);

            let buffer_row = info.buffer_row?;
            let mut cursor = self.buffers.get(&buffer_id)?.entries.cursor::<u32>(());
            cursor.seek_forward(&buffer_row, Bias::Right);
            Some((buffer_id, cursor.item()?.blame.clone()?))
        })
    }

    pub fn max_author_length(&mut self, cx: &mut App) -> usize {
        let mut max_author_length = 0;
        self.sync_all(cx);

        for buffer in self.buffers.values() {
            for entry in buffer.entries.iter() {
                let author_len = entry
                    .blame
                    .as_ref()
                    .and_then(|entry| entry.author.as_ref())
                    .map(|author| author.len());
                if let Some(author_len) = author_len
                    && author_len > max_author_length
                {
                    max_author_length = author_len;
                }
            }
        }

        max_author_length
    }

    pub fn blur(&mut self, _: &mut Context<Self>) {
        self.focused = false;
    }

    pub fn focus(&mut self, cx: &mut Context<Self>) {
        if self.focused {
            return;
        }
        self.focused = true;
        if self.changed_while_blurred {
            self.changed_while_blurred = false;
            self.generate(cx);
        }
    }
}
