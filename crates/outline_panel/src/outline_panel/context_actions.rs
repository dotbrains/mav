use super::*;

impl OutlinePanel {
    pub(super) fn dispatch_context(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> KeyContext {
        let mut dispatch_context = KeyContext::new_with_defaults();
        dispatch_context.add("OutlinePanel");
        dispatch_context.add("menu");
        let identifier = if self.filter_editor.focus_handle(cx).is_focused(window) {
            "editing"
        } else {
            "not_editing"
        };
        dispatch_context.add(identifier);
        dispatch_context
    }

    pub(super) fn deploy_context_menu(
        &mut self,
        position: Point<Pixels>,
        entry: PanelEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_entry(entry.clone(), true, window, cx);
        let is_root = match &entry {
            PanelEntry::Fs(FsEntry::File(FsEntryFile {
                worktree_id, entry, ..
            }))
            | PanelEntry::Fs(FsEntry::Directory(FsEntryDirectory {
                worktree_id, entry, ..
            })) => self
                .project
                .read(cx)
                .worktree_for_id(*worktree_id, cx)
                .map(|worktree| {
                    worktree.read(cx).root_entry().map(|entry| entry.id) == Some(entry.id)
                })
                .unwrap_or(false),
            PanelEntry::FoldedDirs(FoldedDirsEntry {
                worktree_id,
                entries,
                ..
            }) => entries
                .first()
                .and_then(|entry| {
                    self.project
                        .read(cx)
                        .worktree_for_id(*worktree_id, cx)
                        .map(|worktree| {
                            worktree.read(cx).root_entry().map(|entry| entry.id) == Some(entry.id)
                        })
                })
                .unwrap_or(false),
            PanelEntry::Fs(FsEntry::ExternalFile(..)) => false,
            PanelEntry::Outline(..) => {
                cx.notify();
                return;
            }
            PanelEntry::Search(_) => {
                cx.notify();
                return;
            }
        };
        let auto_fold_dirs = OutlinePanelSettings::get_global(cx).auto_fold_dirs;
        let is_foldable = auto_fold_dirs && !is_root && self.is_foldable(&entry);
        let is_unfoldable = auto_fold_dirs && !is_root && self.is_unfoldable(&entry);

        let context_menu = ContextMenu::build(window, cx, |menu, _, _| {
            menu.context(self.focus_handle.clone())
                .action(
                    ui::utils::reveal_in_file_manager_label(false),
                    Box::new(RevealInFileManager),
                )
                .action("Open in Terminal", Box::new(OpenInTerminal))
                .when(is_unfoldable, |menu| {
                    menu.action("Unfold Directory", Box::new(UnfoldDirectory))
                })
                .when(is_foldable, |menu| {
                    menu.action("Fold Directory", Box::new(FoldDirectory))
                })
                .separator()
                .action("Copy Path", Box::new(mav_actions::workspace::CopyPath))
                .action(
                    "Copy Relative Path",
                    Box::new(mav_actions::workspace::CopyRelativePath),
                )
        });
        window.focus(&context_menu.focus_handle(cx), cx);
        let subscription = cx.subscribe(&context_menu, |outline_panel, _, _: &DismissEvent, cx| {
            outline_panel.context_menu.take();
            cx.notify();
        });
        self.context_menu = Some((context_menu, position, subscription));
        cx.notify();
    }

    pub(super) fn is_unfoldable(&self, entry: &PanelEntry) -> bool {
        matches!(entry, PanelEntry::FoldedDirs(..))
    }

    pub(super) fn is_foldable(&self, entry: &PanelEntry) -> bool {
        let (directory_worktree, directory_entry) = match entry {
            PanelEntry::Fs(FsEntry::Directory(FsEntryDirectory {
                worktree_id,
                entry: directory_entry,
                ..
            })) => (*worktree_id, Some(directory_entry)),
            _ => return false,
        };
        let Some(directory_entry) = directory_entry else {
            return false;
        };

        if self
            .unfolded_dirs
            .get(&directory_worktree)
            .is_none_or(|unfolded_dirs| !unfolded_dirs.contains(&directory_entry.id))
        {
            return false;
        }

        let children = self
            .fs_children_count
            .get(&directory_worktree)
            .and_then(|entries| entries.get(&directory_entry.path))
            .copied()
            .unwrap_or_default();

        children.may_be_fold_part() && children.dirs > 0
    }

    pub(super) fn copy_path(
        &mut self,
        _: &mav_actions::workspace::CopyPath,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(clipboard_text) = self
            .selected_entry()
            .and_then(|entry| self.abs_path(entry, cx))
            .map(|p| p.to_string_lossy().into_owned())
        {
            cx.write_to_clipboard(ClipboardItem::new_string(clipboard_text));
        }
    }

    pub(super) fn copy_relative_path(
        &mut self,
        _: &mav_actions::workspace::CopyRelativePath,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let path_style = self.project.read(cx).path_style(cx);
        if let Some(clipboard_text) = self
            .selected_entry()
            .and_then(|entry| match entry {
                PanelEntry::Fs(entry) => self.relative_path(entry, cx),
                PanelEntry::FoldedDirs(folded_dirs) => {
                    folded_dirs.entries.last().map(|entry| entry.path.clone())
                }
                PanelEntry::Search(_) | PanelEntry::Outline(..) => None,
            })
            .map(|p| p.display(path_style).to_string())
        {
            cx.write_to_clipboard(ClipboardItem::new_string(clipboard_text));
        }
    }

    pub(super) fn reveal_in_finder(
        &mut self,
        _: &RevealInFileManager,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(abs_path) = self
            .selected_entry()
            .and_then(|entry| self.abs_path(entry, cx))
        {
            self.project
                .update(cx, |project, cx| project.reveal_path(&abs_path, cx));
        }
    }

    pub(super) fn open_in_terminal(
        &mut self,
        _: &OpenInTerminal,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let selected_entry = self.selected_entry();
        let abs_path = selected_entry.and_then(|entry| self.abs_path(entry, cx));
        let working_directory = if let (
            Some(abs_path),
            Some(PanelEntry::Fs(FsEntry::File(..) | FsEntry::ExternalFile(..))),
        ) = (&abs_path, selected_entry)
        {
            abs_path.parent().map(|p| p.to_owned())
        } else {
            abs_path
        };

        if let Some(working_directory) = working_directory {
            window.dispatch_action(
                workspace::OpenTerminal {
                    working_directory,
                    local: false,
                }
                .boxed_clone(),
                cx,
            )
        }
    }

    pub(super) fn entry_name(&self, worktree_id: &WorktreeId, entry: &Entry, cx: &App) -> String {
        match self.project.read(cx).worktree_for_id(*worktree_id, cx) {
            Some(worktree) => {
                let worktree = worktree.read(cx);
                match worktree.snapshot().root_entry() {
                    Some(root_entry) => {
                        if root_entry.id == entry.id {
                            file_name(worktree.abs_path().as_ref())
                        } else {
                            let path = worktree.absolutize(entry.path.as_ref());
                            file_name(&path)
                        }
                    }
                    None => {
                        let path = worktree.absolutize(entry.path.as_ref());
                        file_name(&path)
                    }
                }
            }
            None => file_name(entry.path.as_std_path()),
        }
    }
}
