use super::*;

impl LanguageServer {
    /// Add new workspace folder to the list.
    pub fn add_workspace_folder(&self, uri: Uri) {
        if self
            .capabilities()
            .workspace
            .and_then(|ws| {
                ws.workspace_folders.and_then(|folders| {
                    folders
                        .change_notifications
                        .map(|caps| matches!(caps, OneOf::Left(false)))
                })
            })
            .unwrap_or(true)
        {
            return;
        }

        let Some(workspace_folders) = self.workspace_folders.as_ref() else {
            return;
        };
        let is_new_folder = workspace_folders.lock().insert(uri.clone());
        if is_new_folder {
            let params = DidChangeWorkspaceFoldersParams {
                event: WorkspaceFoldersChangeEvent {
                    added: vec![WorkspaceFolder {
                        uri,
                        name: String::default(),
                    }],
                    removed: vec![],
                },
            };
            self.notify::<DidChangeWorkspaceFolders>(params).ok();
        }
    }

    /// Remove existing workspace folder from the list.
    pub fn remove_workspace_folder(&self, uri: Uri) {
        if self
            .capabilities()
            .workspace
            .and_then(|ws| {
                ws.workspace_folders.and_then(|folders| {
                    folders
                        .change_notifications
                        .map(|caps| !matches!(caps, OneOf::Left(false)))
                })
            })
            .unwrap_or(true)
        {
            return;
        }
        let Some(workspace_folders) = self.workspace_folders.as_ref() else {
            return;
        };
        let was_removed = workspace_folders.lock().remove(&uri);
        if was_removed {
            let params = DidChangeWorkspaceFoldersParams {
                event: WorkspaceFoldersChangeEvent {
                    added: vec![],
                    removed: vec![WorkspaceFolder {
                        uri,
                        name: String::default(),
                    }],
                },
            };
            self.notify::<DidChangeWorkspaceFolders>(params).ok();
        }
    }
    pub fn set_workspace_folders(&self, folders: BTreeSet<Uri>) {
        let Some(workspace_folders) = self.workspace_folders.as_ref() else {
            return;
        };
        let mut workspace_folders = workspace_folders.lock();

        let old_workspace_folders = std::mem::take(&mut *workspace_folders);
        let added: Vec<_> = folders
            .difference(&old_workspace_folders)
            .map(|uri| WorkspaceFolder {
                uri: uri.clone(),
                name: String::default(),
            })
            .collect();

        let removed: Vec<_> = old_workspace_folders
            .difference(&folders)
            .map(|uri| WorkspaceFolder {
                uri: uri.clone(),
                name: String::default(),
            })
            .collect();
        *workspace_folders = folders;
        let should_notify = !added.is_empty() || !removed.is_empty();
        if should_notify {
            drop(workspace_folders);
            let params = DidChangeWorkspaceFoldersParams {
                event: WorkspaceFoldersChangeEvent { added, removed },
            };
            self.notify::<DidChangeWorkspaceFolders>(params).ok();
        }
    }

    pub fn workspace_folders(&self) -> BTreeSet<Uri> {
        self.workspace_folders.as_ref().map_or_else(
            || BTreeSet::from_iter([self.root_uri.clone()]),
            |folders| folders.lock().clone(),
        )
    }

    pub fn register_buffer(
        &self,
        uri: Uri,
        language_id: String,
        version: i32,
        initial_text: String,
    ) {
        self.notify::<notification::DidOpenTextDocument>(DidOpenTextDocumentParams {
            text_document: TextDocumentItem::new(uri, language_id, version, initial_text),
        })
        .ok();
    }

    pub fn unregister_buffer(&self, uri: Uri) {
        self.notify::<notification::DidCloseTextDocument>(DidCloseTextDocumentParams {
            text_document: TextDocumentIdentifier::new(uri),
        })
        .ok();
    }
}
