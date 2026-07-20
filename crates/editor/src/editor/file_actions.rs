use super::*;

impl Editor {
    pub(super) fn target_file<'a>(&self, cx: &'a App) -> Option<&'a dyn language::LocalFile> {
        self.active_buffer(cx)?
            .read(cx)
            .file()
            .and_then(|f| f.as_local())
    }

    pub(super) fn reveal_in_finder(
        &mut self,
        _: &RevealInFileManager,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(path) = self.target_file_abs_path(cx) {
            if let Some(project) = self.project() {
                project.update(cx, |project, cx| project.reveal_path(&path, cx));
            } else {
                cx.reveal_path(&path);
            }
        }
    }

    pub(super) fn copy_path(
        &mut self,
        _: &mav_actions::workspace::CopyPath,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(path) = self.target_file_abs_path(cx)
            && let Some(path) = path.to_str()
        {
            cx.write_to_clipboard(ClipboardItem::new_string(path.to_string()));
        } else {
            cx.propagate();
        }
    }

    pub(super) fn copy_relative_path(
        &mut self,
        _: &mav_actions::workspace::CopyRelativePath,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(path) = self.active_buffer(cx).and_then(|buffer| {
            let project = self.project()?.read(cx);
            let path = buffer.read(cx).file()?.path();
            let path = path.display(project.path_style(cx));
            Some(path)
        }) {
            cx.write_to_clipboard(ClipboardItem::new_string(path.to_string()));
        } else {
            cx.propagate();
        }
    }

    pub fn copy_file_name_without_extension(
        &mut self,
        _: &CopyFileNameWithoutExtension,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(file_stem) = self.active_buffer(cx).and_then(|buffer| {
            let file = buffer.read(cx).file()?;
            file.path().file_stem()
        }) {
            cx.write_to_clipboard(ClipboardItem::new_string(file_stem.to_string()));
        }
    }

    pub fn copy_file_name(&mut self, _: &CopyFileName, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(file_name) = self.active_buffer(cx).and_then(|buffer| {
            let file = buffer.read(cx).file()?;
            Some(file.file_name(cx))
        }) {
            cx.write_to_clipboard(ClipboardItem::new_string(file_name.to_string()));
        }
    }

    pub fn copy_file_location(
        &mut self,
        _: &CopyFileLocation,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let selection = self.selections.newest::<Point>(&self.display_snapshot(cx));

        let start_line = selection.start.row + 1;
        let end_line = selection.end.row + 1;

        let end_line = if selection.end.column == 0 && end_line > start_line {
            end_line - 1
        } else {
            end_line
        };

        if let Some(file_location) = self.active_buffer(cx).and_then(|buffer| {
            let project = self.project()?.read(cx);
            let file = buffer.read(cx).file()?;
            let path = file.path().display(project.path_style(cx));

            let location = if start_line == end_line {
                format!("{path}:{start_line}")
            } else {
                format!("{path}:{start_line}-{end_line}")
            };
            Some(location)
        }) {
            cx.write_to_clipboard(ClipboardItem::new_string(file_location));
        }
    }

    pub fn insert_uuid_v4(
        &mut self,
        _: &InsertUuidV4,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.insert_uuid(UuidVersion::V4, window, cx);
    }

    pub fn insert_uuid_v7(
        &mut self,
        _: &InsertUuidV7,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.insert_uuid(UuidVersion::V7, window, cx);
    }

    fn insert_uuid(&mut self, version: UuidVersion, window: &mut Window, cx: &mut Context<Self>) {
        if self.read_only(cx) {
            return;
        }
        self.transact(window, cx, |this, window, cx| {
            let edits = this
                .selections
                .all::<Point>(&this.display_snapshot(cx))
                .into_iter()
                .map(|selection| {
                    let uuid = match version {
                        UuidVersion::V4 => uuid::Uuid::new_v4(),
                        UuidVersion::V7 => uuid::Uuid::now_v7(),
                    };

                    (selection.range(), uuid.to_string())
                });
            this.edit(edits, cx);
            this.refresh_edit_prediction(
                true,
                false,
                EditPredictionRequestTrigger::BufferEdit,
                window,
                cx,
            );
        });
    }
}
