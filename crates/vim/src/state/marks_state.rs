use super::*;

impl MarksState {
    pub fn new(workspace: &Workspace, cx: &mut App) -> Entity<MarksState> {
        cx.new(|cx| {
            let buffer_store = workspace.project().read(cx).buffer_store().clone();
            let subscription = cx.subscribe(&buffer_store, move |this: &mut Self, _, event, cx| {
                if let project::buffer_store::BufferStoreEvent::BufferAdded(buffer) = event {
                    this.on_buffer_loaded(buffer, cx);
                }
            });

            let mut this = Self {
                workspace: workspace.weak_handle(),
                multibuffer_marks: HashMap::default(),
                buffer_marks: HashMap::default(),
                watched_buffers: HashMap::default(),
                serialized_marks: HashMap::default(),
                global_marks: HashMap::default(),
                _subscription: subscription,
            };

            this.load(cx);
            this
        })
    }

    fn workspace_id(&self, cx: &App) -> Option<WorkspaceId> {
        self.workspace
            .read_with(cx, |workspace, _| workspace.database_id())
            .ok()
            .flatten()
    }

    fn project(&self, cx: &App) -> Option<Entity<Project>> {
        self.workspace
            .read_with(cx, |workspace, _| workspace.project().clone())
            .ok()
    }

    fn load(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let Some(workspace_id) = this.update(cx, |this, cx| this.workspace_id(cx)).ok()? else {
                return None;
            };
            let db = cx.update(|cx| VimDb::global(cx));
            let (marks, paths) = cx
                .background_spawn(async move {
                    let marks = db.get_marks(workspace_id)?;
                    let paths = db.get_global_marks_paths(workspace_id)?;
                    anyhow::Ok((marks, paths))
                })
                .await
                .log_err()?;
            this.update(cx, |this, cx| this.loaded(marks, paths, cx))
                .ok()
        })
        .detach();
    }

    fn loaded(
        &mut self,
        marks: Vec<SerializedMark>,
        global_mark_paths: Vec<(String, Arc<Path>)>,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = self.project(cx) else {
            return;
        };

        for mark in marks {
            self.serialized_marks
                .entry(mark.path)
                .or_default()
                .insert(mark.name, mark.points);
        }

        for (name, path) in global_mark_paths {
            self.global_marks
                .insert(name, MarkLocation::Path(path.clone()));

            let project_path = project
                .read(cx)
                .worktrees(cx)
                .filter_map(|worktree| {
                    let relative = path.strip_prefix(worktree.read(cx).abs_path()).ok()?;
                    let path = RelPath::new(relative, worktree.read(cx).path_style()).log_err()?;
                    Some(ProjectPath {
                        worktree_id: worktree.read(cx).id(),
                        path: path.into_arc(),
                    })
                })
                .next();
            if let Some(buffer) = project_path
                .and_then(|project_path| project.read(cx).get_open_buffer(&project_path, cx))
            {
                self.on_buffer_loaded(&buffer, cx)
            }
        }
    }

    pub fn on_buffer_loaded(&mut self, buffer_handle: &Entity<Buffer>, cx: &mut Context<Self>) {
        let Some(project) = self.project(cx) else {
            return;
        };
        let Some(project_path) = buffer_handle.read(cx).project_path(cx) else {
            return;
        };
        let Some(abs_path) = project.read(cx).absolute_path(&project_path, cx) else {
            return;
        };
        let abs_path: Arc<Path> = abs_path.into();

        let Some(serialized_marks) = self.serialized_marks.get(&abs_path) else {
            return;
        };

        let mut loaded_marks = HashMap::default();
        let buffer = buffer_handle.read(cx);
        for (name, points) in serialized_marks.iter() {
            loaded_marks.insert(
                name.clone(),
                points
                    .iter()
                    .map(|point| buffer.anchor_before(buffer.clip_point(*point, Bias::Left)))
                    .collect(),
            );
        }
        self.buffer_marks.insert(buffer.remote_id(), loaded_marks);
        self.watch_buffer(MarkLocation::Path(abs_path), buffer_handle, cx)
    }

    fn serialize_buffer_marks(
        &mut self,
        path: Arc<Path>,
        buffer: &Entity<Buffer>,
        cx: &mut Context<Self>,
    ) {
        let new_points: HashMap<String, Vec<Point>> =
            if let Some(anchors) = self.buffer_marks.get(&buffer.read(cx).remote_id()) {
                anchors
                    .iter()
                    .map(|(name, anchors)| {
                        (
                            name.clone(),
                            buffer
                                .read(cx)
                                .summaries_for_anchors::<Point, _>(anchors.iter().copied())
                                .collect(),
                        )
                    })
                    .collect()
            } else {
                HashMap::default()
            };
        let old_points = self.serialized_marks.get(&path);
        if old_points == Some(&new_points) {
            return;
        }
        let mut to_write = HashMap::default();

        for (key, value) in &new_points {
            if self.is_global_mark(key)
                && self.global_marks.get(key) != Some(&MarkLocation::Path(path.clone()))
            {
                if let Some(workspace_id) = self.workspace_id(cx) {
                    let path = path.clone();
                    let key = key.clone();
                    let db = VimDb::global(cx);
                    cx.background_spawn(async move {
                        db.set_global_mark_path(workspace_id, key, path).await
                    })
                    .detach_and_log_err(cx);
                }

                self.global_marks
                    .insert(key.clone(), MarkLocation::Path(path.clone()));
            }
            if old_points.and_then(|o| o.get(key)) != Some(value) {
                to_write.insert(key.clone(), value.clone());
            }
        }

        self.serialized_marks.insert(path.clone(), new_points);

        if let Some(workspace_id) = self.workspace_id(cx) {
            let db = VimDb::global(cx);
            cx.background_spawn(async move {
                db.set_marks(workspace_id, path.clone(), to_write).await?;
                anyhow::Ok(())
            })
            .detach_and_log_err(cx);
        }
    }

    fn is_global_mark(&self, key: &str) -> bool {
        key.chars()
            .next()
            .is_some_and(|c| c.is_uppercase() || c.is_digit(10))
    }

    fn rename_buffer(
        &mut self,
        old_path: MarkLocation,
        new_path: Arc<Path>,
        buffer: &Entity<Buffer>,
        cx: &mut Context<Self>,
    ) {
        if let MarkLocation::Buffer(entity_id) = old_path
            && let Some(old_marks) = self.multibuffer_marks.remove(&entity_id)
        {
            let buffer_marks = old_marks
                .into_iter()
                .map(|(k, v)| {
                    (
                        k,
                        v.into_iter()
                            .filter_map(|anchor| anchor.raw_text_anchor())
                            .collect(),
                    )
                })
                .collect();
            self.buffer_marks
                .insert(buffer.read(cx).remote_id(), buffer_marks);
        }
        self.watch_buffer(MarkLocation::Path(new_path.clone()), buffer, cx);
        self.serialize_buffer_marks(new_path, buffer, cx);
    }

    fn path_for_buffer(&self, buffer: &Entity<Buffer>, cx: &App) -> Option<Arc<Path>> {
        let project_path = buffer.read(cx).project_path(cx)?;
        let project = self.project(cx)?;
        let abs_path = project.read(cx).absolute_path(&project_path, cx)?;
        Some(abs_path.into())
    }

    fn points_at(
        &self,
        location: &MarkLocation,
        multi_buffer: &Entity<MultiBuffer>,
        cx: &App,
    ) -> bool {
        match location {
            MarkLocation::Buffer(entity_id) => entity_id == &multi_buffer.entity_id(),
            MarkLocation::Path(path) => {
                let Some(singleton) = multi_buffer.read(cx).as_singleton() else {
                    return false;
                };
                self.path_for_buffer(&singleton, cx).as_ref() == Some(path)
            }
        }
    }

    pub fn watch_buffer(
        &mut self,
        mark_location: MarkLocation,
        buffer_handle: &Entity<Buffer>,
        cx: &mut Context<Self>,
    ) {
        let on_change = cx.subscribe(buffer_handle, move |this, buffer, event, cx| match event {
            BufferEvent::Edited { .. } => {
                if let Some(path) = this.path_for_buffer(&buffer, cx) {
                    this.serialize_buffer_marks(path, &buffer, cx);
                }
            }
            BufferEvent::FileHandleChanged => {
                let buffer_id = buffer.read(cx).remote_id();
                if let Some(old_path) = this
                    .watched_buffers
                    .get(&buffer_id.clone())
                    .map(|(path, _, _)| path.clone())
                    && let Some(new_path) = this.path_for_buffer(&buffer, cx)
                {
                    this.rename_buffer(old_path, new_path, &buffer, cx)
                }
            }
            _ => {}
        });

        let on_release = cx.observe_release(buffer_handle, |this, buffer, _| {
            this.watched_buffers.remove(&buffer.remote_id());
            this.buffer_marks.remove(&buffer.remote_id());
        });

        self.watched_buffers.insert(
            buffer_handle.read(cx).remote_id(),
            (mark_location, on_change, on_release),
        );
    }

    pub fn set_mark(
        &mut self,
        name: String,
        multibuffer: &Entity<MultiBuffer>,
        anchors: Vec<Anchor>,
        cx: &mut Context<Self>,
    ) {
        let multibuffer_snapshot = multibuffer.read(cx).snapshot(cx);
        let buffer = multibuffer.read(cx).as_singleton();
        let abs_path = buffer.as_ref().and_then(|b| self.path_for_buffer(b, cx));

        if self.is_global_mark(&name) && self.global_marks.contains_key(&name) {
            self.delete_mark(name.clone(), multibuffer, cx);
        }

        let Some(abs_path) = abs_path else {
            self.multibuffer_marks
                .entry(multibuffer.entity_id())
                .or_default()
                .insert(name.clone(), anchors);
            if self.is_global_mark(&name) {
                self.global_marks
                    .insert(name, MarkLocation::Buffer(multibuffer.entity_id()));
            }
            if let Some(buffer) = buffer {
                let buffer_id = buffer.read(cx).remote_id();
                if !self.watched_buffers.contains_key(&buffer_id) {
                    self.watch_buffer(MarkLocation::Buffer(multibuffer.entity_id()), &buffer, cx)
                }
            }
            return;
        };
        let Some(buffer) = buffer else {
            return;
        };

        let buffer_id = buffer.read(cx).remote_id();
        self.buffer_marks.entry(buffer_id).or_default().insert(
            name.clone(),
            anchors
                .into_iter()
                .filter_map(|anchor| Some(multibuffer_snapshot.anchor_to_buffer_anchor(anchor)?.0))
                .collect(),
        );
        if !self.watched_buffers.contains_key(&buffer_id) {
            self.watch_buffer(MarkLocation::Path(abs_path.clone()), &buffer, cx)
        }
        if self.is_global_mark(&name) {
            self.global_marks
                .insert(name, MarkLocation::Path(abs_path.clone()));
        }
        self.serialize_buffer_marks(abs_path, &buffer, cx)
    }

    pub fn get_mark(
        &self,
        name: &str,
        multi_buffer: &Entity<MultiBuffer>,
        cx: &App,
    ) -> Option<Mark> {
        let target = self.global_marks.get(name);

        if !self.is_global_mark(name) || target.is_some_and(|t| self.points_at(t, multi_buffer, cx))
        {
            if let Some(anchors) = self.multibuffer_marks.get(&multi_buffer.entity_id()) {
                return Some(Mark::Local(anchors.get(name)?.clone()));
            }

            let multibuffer_snapshot = multi_buffer.read(cx).snapshot(cx);
            let buffer_snapshot = multibuffer_snapshot.as_singleton()?;
            if let Some(anchors) = self.buffer_marks.get(&buffer_snapshot.remote_id()) {
                let text_anchors = anchors.get(name)?;
                let anchors = text_anchors
                    .iter()
                    .filter_map(|anchor| multibuffer_snapshot.anchor_in_excerpt(*anchor))
                    .collect();
                return Some(Mark::Local(anchors));
            }
        }

        match target? {
            MarkLocation::Buffer(entity_id) => {
                let anchors = self.multibuffer_marks.get(entity_id)?;
                Some(Mark::Buffer(*entity_id, anchors.get(name)?.clone()))
            }
            MarkLocation::Path(path) => {
                let points = self.serialized_marks.get(path)?;
                Some(Mark::Path(path.clone(), points.get(name)?.clone()))
            }
        }
    }
    pub fn delete_mark(
        &mut self,
        mark_name: String,
        multi_buffer: &Entity<MultiBuffer>,
        cx: &mut Context<Self>,
    ) {
        let path = if let Some(target) = self.global_marks.get(&mark_name.clone()) {
            let name = mark_name.clone();
            if let Some(workspace_id) = self.workspace_id(cx) {
                let db = VimDb::global(cx);
                cx.background_spawn(async move {
                    db.delete_global_marks_path(workspace_id, name).await
                })
                .detach_and_log_err(cx);
            }
            self.buffer_marks.iter_mut().for_each(|(_, m)| {
                m.remove(&mark_name.clone());
            });

            match target {
                MarkLocation::Buffer(entity_id) => {
                    self.multibuffer_marks
                        .get_mut(entity_id)
                        .map(|m| m.remove(&mark_name.clone()));
                    return;
                }
                MarkLocation::Path(path) => path.clone(),
            }
        } else {
            self.multibuffer_marks
                .get_mut(&multi_buffer.entity_id())
                .map(|m| m.remove(&mark_name.clone()));

            if let Some(singleton) = multi_buffer.read(cx).as_singleton() {
                let buffer_id = singleton.read(cx).remote_id();
                self.buffer_marks
                    .get_mut(&buffer_id)
                    .map(|m| m.remove(&mark_name.clone()));
                let Some(path) = self.path_for_buffer(&singleton, cx) else {
                    return;
                };
                path
            } else {
                return;
            }
        };
        self.global_marks.remove(&mark_name);
        self.serialized_marks
            .get_mut(&path)
            .map(|m| m.remove(&mark_name.clone()));
        if let Some(workspace_id) = self.workspace_id(cx) {
            let db = VimDb::global(cx);
            cx.background_spawn(async move { db.delete_mark(workspace_id, path, mark_name).await })
                .detach_and_log_err(cx);
        }
    }
}
