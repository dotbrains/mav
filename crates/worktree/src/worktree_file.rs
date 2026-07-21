use super::*;

#[derive(Debug, Clone, PartialEq)]
pub struct File {
    pub worktree: Entity<Worktree>,
    pub path: Arc<RelPath>,
    pub disk_state: DiskState,
    pub entry_id: Option<ProjectEntryId>,
    pub is_local: bool,
    pub is_private: bool,
}

impl language::File for File {
    fn as_local(&self) -> Option<&dyn language::LocalFile> {
        if self.is_local { Some(self) } else { None }
    }

    fn disk_state(&self) -> DiskState {
        self.disk_state
    }

    fn path(&self) -> &Arc<RelPath> {
        &self.path
    }

    fn full_path(&self, cx: &App) -> PathBuf {
        self.worktree.read(cx).full_path(&self.path)
    }

    /// Returns the last component of this handle's absolute path. If this handle refers to the root
    /// of its worktree, then this method will return the name of the worktree itself.
    fn file_name<'a>(&'a self, cx: &'a App) -> &'a str {
        self.path
            .file_name()
            .unwrap_or_else(|| self.worktree.read(cx).root_name_str())
    }

    fn worktree_id(&self, cx: &App) -> WorktreeId {
        self.worktree.read(cx).id()
    }

    fn to_proto(&self, cx: &App) -> rpc::proto::File {
        rpc::proto::File {
            worktree_id: self.worktree.read(cx).id().to_proto(),
            entry_id: self.entry_id.map(|id| id.to_proto()),
            path: self.path.as_ref().to_proto(),
            mtime: self.disk_state.mtime().map(|time| time.into()),
            is_deleted: self.disk_state.is_deleted(),
            is_historic: matches!(self.disk_state, DiskState::Historic { .. }),
        }
    }

    fn is_private(&self) -> bool {
        self.is_private
    }

    fn path_style(&self, cx: &App) -> PathStyle {
        self.worktree.read(cx).path_style()
    }

    fn can_open(&self) -> bool {
        true
    }
}

impl language::LocalFile for File {
    fn abs_path(&self, cx: &App) -> PathBuf {
        self.worktree.read(cx).absolutize(&self.path)
    }

    fn load(&self, cx: &App) -> Task<Result<String>> {
        let worktree = self.worktree.read(cx).as_local().unwrap();
        let abs_path = worktree.absolutize(&self.path);
        let fs = worktree.fs.clone();
        cx.background_spawn(async move { fs.load(&abs_path).await })
    }

    fn load_bytes(&self, cx: &App) -> Task<Result<Vec<u8>>> {
        let worktree = self.worktree.read(cx).as_local().unwrap();
        let abs_path = worktree.absolutize(&self.path);
        let fs = worktree.fs.clone();
        cx.background_spawn(async move { fs.load_bytes(&abs_path).await })
    }
}

impl File {
    pub fn for_entry(entry: Entry, worktree: Entity<Worktree>) -> Arc<Self> {
        Arc::new(Self {
            worktree,
            path: entry.path.clone(),
            disk_state: if let Some(mtime) = entry.mtime {
                DiskState::Present {
                    mtime,
                    size: entry.size,
                }
            } else {
                DiskState::New
            },
            entry_id: Some(entry.id),
            is_local: true,
            is_private: entry.is_private,
        })
    }

    pub fn from_proto(
        proto: rpc::proto::File,
        worktree: Entity<Worktree>,
        cx: &App,
    ) -> Result<Self> {
        let worktree_id = worktree.read(cx).as_remote().context("not remote")?.id();

        anyhow::ensure!(
            worktree_id.to_proto() == proto.worktree_id,
            "worktree id does not match file"
        );

        let disk_state = if proto.is_historic {
            DiskState::Historic {
                was_deleted: proto.is_deleted,
            }
        } else if proto.is_deleted {
            DiskState::Deleted
        } else if let Some(mtime) = proto.mtime.map(&Into::into) {
            DiskState::Present { mtime, size: 0 }
        } else {
            DiskState::New
        };

        Ok(Self {
            worktree,
            path: RelPath::from_proto(&proto.path).context("invalid path in file protobuf")?,
            disk_state,
            entry_id: proto.entry_id.map(ProjectEntryId::from_proto),
            is_local: false,
            is_private: false,
        })
    }

    pub fn from_dyn(file: Option<&Arc<dyn language::File>>) -> Option<&Self> {
        file.and_then(|f| {
            let f: &dyn language::File = f.borrow();
            let f: &dyn Any = f;
            f.downcast_ref()
        })
    }

    pub fn worktree_id(&self, cx: &App) -> WorktreeId {
        self.worktree.read(cx).id()
    }

    pub fn project_entry_id(&self) -> Option<ProjectEntryId> {
        match self.disk_state {
            DiskState::Deleted => None,
            _ => self.entry_id,
        }
    }
}
