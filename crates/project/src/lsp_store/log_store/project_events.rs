use gpui::{Context, Entity};

use crate::{Project, ProjectItem as _};

use super::{LanguageServerKind, LogStore, MessageKind, ProjectState};

impl LogStore {
    pub fn add_project(&mut self, project: &Entity<Project>, cx: &mut Context<Self>) {
        let weak_project = project.downgrade();
        self.projects.insert(
            project.downgrade(),
            ProjectState {
                _subscriptions: [
                    cx.observe_release(project, move |this, _, _| {
                        this.projects.remove(&weak_project);
                        this.language_servers
                            .retain(|_, state| state.kind.project() != Some(&weak_project));
                    }),
                    cx.subscribe(project, move |log_store, project, event, cx| {
                        let server_kind = if project.read(cx).is_local() {
                            LanguageServerKind::Local {
                                project: project.downgrade(),
                            }
                        } else {
                            LanguageServerKind::Remote {
                                project: project.downgrade(),
                            }
                        };
                        match event {
                            crate::Event::LanguageServerAdded(id, name, worktree_id) => {
                                log_store.add_language_server(
                                    server_kind,
                                    *id,
                                    Some(name.clone()),
                                    *worktree_id,
                                    project
                                        .read(cx)
                                        .lsp_store()
                                        .read(cx)
                                        .language_server_for_id(*id),
                                    cx,
                                );
                            }
                            crate::Event::LanguageServerBufferRegistered {
                                server_id,
                                buffer_id,
                                name,
                                ..
                            } => {
                                let worktree_id = project
                                    .read(cx)
                                    .buffer_for_id(*buffer_id, cx)
                                    .and_then(|buffer| {
                                        Some(buffer.read(cx).project_path(cx)?.worktree_id)
                                    });
                                let name = name.clone().or_else(|| {
                                    project
                                        .read(cx)
                                        .lsp_store()
                                        .read(cx)
                                        .language_server_statuses
                                        .get(server_id)
                                        .map(|status| status.name.clone())
                                });
                                log_store.add_language_server(
                                    server_kind,
                                    *server_id,
                                    name,
                                    worktree_id,
                                    None,
                                    cx,
                                );
                            }
                            crate::Event::LanguageServerRemoved(id) => {
                                log_store.remove_language_server(*id, cx);
                            }
                            crate::Event::LanguageServerLog(id, typ, message) => {
                                log_store.add_language_server(
                                    server_kind,
                                    *id,
                                    None,
                                    None,
                                    None,
                                    cx,
                                );
                                match typ {
                                    crate::LanguageServerLogType::Log(typ) => {
                                        log_store.add_language_server_log(*id, *typ, message, cx);
                                    }
                                    crate::LanguageServerLogType::Trace { verbose_info } => {
                                        log_store.add_language_server_trace(
                                            *id,
                                            message,
                                            verbose_info.clone(),
                                            cx,
                                        );
                                    }
                                    crate::LanguageServerLogType::Rpc { received } => {
                                        let kind = if *received {
                                            MessageKind::Receive
                                        } else {
                                            MessageKind::Send
                                        };
                                        log_store.add_language_server_rpc(*id, kind, message, cx);
                                    }
                                }
                            }
                            crate::Event::ToggleLspLogs {
                                server_id,
                                enabled,
                                toggled_log_kind,
                            } => {
                                log_store.toggle_lsp_logs(*server_id, *enabled, *toggled_log_kind);
                            }
                            _ => {}
                        }
                    }),
                ],
                copilot_log_subscription: None,
            },
        );
    }
}
