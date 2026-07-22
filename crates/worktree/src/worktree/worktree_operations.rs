use super::*;

impl Worktree {
    pub fn load_file(&self, path: &RelPath, cx: &Context<Worktree>) -> Task<Result<LoadedFile>> {
        match self {
            Worktree::Local(this) => this.load_file(path, cx),
            Worktree::Remote(_) => {
                Task::ready(Err(anyhow!("remote worktrees can't yet load files")))
            }
        }
    }

    pub fn load_binary_file(
        &self,
        path: &RelPath,
        cx: &Context<Worktree>,
    ) -> Task<Result<LoadedBinaryFile>> {
        match self {
            Worktree::Local(this) => this.load_binary_file(path, cx),
            Worktree::Remote(_) => {
                Task::ready(Err(anyhow!("remote worktrees can't yet load binary files")))
            }
        }
    }

    pub fn write_file(
        &self,
        path: Arc<RelPath>,
        text: Rope,
        line_ending: LineEnding,
        encoding: &'static Encoding,
        has_bom: bool,
        cx: &Context<Worktree>,
    ) -> Task<Result<Arc<File>>> {
        match self {
            Worktree::Local(this) => {
                this.write_file(path, text, line_ending, encoding, has_bom, cx)
            }
            Worktree::Remote(_) => {
                Task::ready(Err(anyhow!("remote worktree can't yet write files")))
            }
        }
    }

    pub fn create_entry(
        &mut self,
        path: Arc<RelPath>,
        is_directory: bool,
        content: Option<Vec<u8>>,
        cx: &Context<Worktree>,
    ) -> Task<Result<CreatedEntry>> {
        let worktree_id = self.id();
        match self {
            Worktree::Local(this) => this.create_entry(path, is_directory, content, cx),
            Worktree::Remote(this) => {
                let project_id = this.project_id;
                let request = this.client.request(proto::CreateProjectEntry {
                    worktree_id: worktree_id.to_proto(),
                    project_id,
                    path: path.as_ref().to_proto(),
                    content,
                    is_directory,
                });
                cx.spawn(async move |this, cx| {
                    let response = request.await?;
                    match response.entry {
                        Some(entry) => this
                            .update(cx, |worktree, cx| {
                                worktree.as_remote_mut().unwrap().insert_entry(
                                    entry,
                                    response.worktree_scan_id as usize,
                                    cx,
                                )
                            })?
                            .await
                            .map(CreatedEntry::Included),
                        None => {
                            let abs_path =
                                this.read_with(cx, |worktree, _| worktree.absolutize(&path))?;
                            Ok(CreatedEntry::Excluded { abs_path })
                        }
                    }
                })
            }
        }
    }

    pub fn delete_entry(
        &mut self,
        entry_id: ProjectEntryId,
        trash: bool,
        cx: &mut Context<Worktree>,
    ) -> Option<Task<Result<Option<TrashedEntry>>>> {
        let task = match self {
            Worktree::Local(this) => this.delete_entry(entry_id, trash, cx),
            Worktree::Remote(this) => this.delete_entry(entry_id, trash, cx),
        }?;

        let entry = match &*self {
            Worktree::Local(this) => this.entry_for_id(entry_id),
            Worktree::Remote(this) => this.entry_for_id(entry_id),
        }?;

        let mut ids = vec![entry_id];
        let path = &*entry.path;

        self.get_children_ids_recursive(path, &mut ids);

        for id in ids {
            cx.emit(Event::DeletedEntry(id));
        }
        Some(task)
    }

    pub async fn restore_entry(
        trash_entry: TrashedEntry,
        worktree: Entity<Self>,
        cx: &mut AsyncApp,
    ) -> Result<RelPathBuf> {
        let is_local = worktree.read_with(cx, |this, _| this.is_local());
        if is_local {
            LocalWorktree::restore_entry(trash_entry, worktree, cx).await
        } else {
            // TODO(dino): Add support for restoring entries in remote worktrees.
            Err(anyhow!("Unsupported"))
        }
    }

    pub(super) fn get_children_ids_recursive(&self, path: &RelPath, ids: &mut Vec<ProjectEntryId>) {
        let children_iter = self.child_entries(path);
        for child in children_iter {
            ids.push(child.id);
            self.get_children_ids_recursive(&child.path, ids);
        }
    }

    // pub fn rename_entry(
    //     &mut self,
    //     entry_id: ProjectEntryId,
    //     new_path: Arc<RelPath>,
    //     cx: &Context<Self>,
    // ) -> Task<Result<CreatedEntry>> {
    //     match self {
    //         Worktree::Local(this) => this.rename_entry(entry_id, new_path, cx),
    //         Worktree::Remote(this) => this.rename_entry(entry_id, new_path, cx),
    //     }
    // }

    pub fn copy_external_entries(
        &mut self,
        target_directory: Arc<RelPath>,
        paths: Vec<Arc<Path>>,
        fs: Arc<dyn Fs>,
        cx: &Context<Worktree>,
    ) -> Task<Result<Vec<ProjectEntryId>>> {
        match self {
            Worktree::Local(this) => this.copy_external_entries(target_directory, paths, cx),
            Worktree::Remote(this) => this.copy_external_entries(target_directory, paths, fs, cx),
        }
    }

    pub fn expand_entry(
        &mut self,
        entry_id: ProjectEntryId,
        cx: &Context<Worktree>,
    ) -> Option<Task<Result<()>>> {
        match self {
            Worktree::Local(this) => this.expand_entry(entry_id, cx),
            Worktree::Remote(this) => {
                let response = this.client.request(proto::ExpandProjectEntry {
                    project_id: this.project_id,
                    entry_id: entry_id.to_proto(),
                });
                Some(cx.spawn(async move |this, cx| {
                    let response = response.await?;
                    this.update(cx, |this, _| {
                        this.as_remote_mut()
                            .unwrap()
                            .wait_for_snapshot(response.worktree_scan_id as usize)
                    })?
                    .await?;
                    Ok(())
                }))
            }
        }
    }

    pub fn expand_all_for_entry(
        &mut self,
        entry_id: ProjectEntryId,
        cx: &Context<Worktree>,
    ) -> Option<Task<Result<()>>> {
        match self {
            Worktree::Local(this) => this.expand_all_for_entry(entry_id, cx),
            Worktree::Remote(this) => {
                let response = this.client.request(proto::ExpandAllForProjectEntry {
                    project_id: this.project_id,
                    entry_id: entry_id.to_proto(),
                });
                Some(cx.spawn(async move |this, cx| {
                    let response = response.await?;
                    this.update(cx, |this, _| {
                        this.as_remote_mut()
                            .unwrap()
                            .wait_for_snapshot(response.worktree_scan_id as usize)
                    })?
                    .await?;
                    Ok(())
                }))
            }
        }
    }
}
