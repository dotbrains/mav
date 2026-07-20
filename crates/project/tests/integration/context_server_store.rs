pub(crate) use anyhow::Result;
pub(crate) use context_server::test::create_fake_transport;
pub(crate) use context_server::{ContextServer, ContextServerId};
pub(crate) use gpui::{
    AppContext, AsyncApp, Entity, Subscription, Task, TestAppContext, UpdateGlobal as _,
};
pub(crate) use http_client::{FakeHttpClient, Response};
pub(crate) use project::context_server_store::registry::ContextServerDescriptorRegistry;
pub(crate) use project::context_server_store::*;
pub(crate) use project::project_settings::ContextServerSettings;
pub(crate) use project::worktree_store::WorktreeStore;
pub(crate) use project::{
    DisableAiSettings, FakeFs, Project, context_server_store::registry::ContextServerDescriptor,
    project_settings::ProjectSettings,
};
pub(crate) use serde_json::json;
pub(crate) use settings::settings_content::SaturatingBool;
pub(crate) use settings::{ContextServerCommand, Settings, SettingsStore};
pub(crate) use std::sync::Arc;
pub(crate) use std::{cell::RefCell, path::PathBuf, rc::Rc};
pub(crate) use util::path;

pub(crate) fn set_context_server_configuration(
    context_servers: Vec<(Arc<str>, settings::ContextServerSettingsContent)>,
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |content| {
                content.project.context_servers.clear();
                for (id, config) in context_servers {
                    content.project.context_servers.insert(id, config);
                }
            });
        })
    });
}

pub(crate) struct ServerEvents {
    received_event_count: Rc<RefCell<usize>>,
    expected_event_count: usize,
    _subscription: Subscription,
}

impl Drop for ServerEvents {
    fn drop(&mut self) {
        let actual_event_count = *self.received_event_count.borrow();
        assert_eq!(
            actual_event_count, self.expected_event_count,
            "
               Expected to receive {} context server store events, but received {} events",
            self.expected_event_count, actual_event_count
        );
    }
}

pub(crate) fn assert_server_events(
    store: &Entity<ContextServerStore>,
    expected_events: Vec<(ContextServerId, ContextServerStatus)>,
    cx: &mut TestAppContext,
) -> ServerEvents {
    cx.update(|cx| {
        let mut ix = 0;
        let received_event_count = Rc::new(RefCell::new(0));
        let expected_event_count = expected_events.len();
        let subscription = cx.subscribe(store, {
            let received_event_count = received_event_count.clone();
            move |_, event, _| {
                let ServerStatusChangedEvent {
                    server_id: actual_server_id,
                    status: actual_status,
                } = event;
                let (expected_server_id, expected_status) = &expected_events[ix];

                assert_eq!(
                    actual_server_id, expected_server_id,
                    "Expected different server id at index {}",
                    ix
                );
                assert_eq!(
                    actual_status, expected_status,
                    "Expected different status at index {}",
                    ix
                );
                ix += 1;
                *received_event_count.borrow_mut() += 1;
            }
        });
        ServerEvents {
            expected_event_count,
            received_event_count,
            _subscription: subscription,
        }
    })
}

pub(crate) async fn setup_context_server_test(
    cx: &mut TestAppContext,
    files: serde_json::Value,
    context_server_configurations: Vec<(Arc<str>, ContextServerSettings)>,
) -> (Arc<FakeFs>, Entity<Project>) {
    cx.update(|cx| {
        let settings_store = SettingsStore::test(cx);
        cx.set_global(settings_store);
        let mut settings = ProjectSettings::get_global(cx).clone();
        for (id, config) in context_server_configurations {
            settings.context_servers.insert(id, config);
        }
        ProjectSettings::override_global(settings, cx);
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/test"), files).await;
    let project = Project::test(fs.clone(), [path!("/test").as_ref()], cx).await;

    (fs, project)
}

pub(crate) struct FakeContextServerDescriptor {
    path: PathBuf,
}

impl FakeContextServerDescriptor {
    pub(crate) fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

impl ContextServerDescriptor for FakeContextServerDescriptor {
    fn command(
        &self,
        _worktree_store: Entity<WorktreeStore>,
        _cx: &AsyncApp,
    ) -> Task<Result<ContextServerCommand>> {
        Task::ready(Ok(ContextServerCommand {
            path: self.path.clone(),
            args: vec!["arg1".to_string(), "arg2".to_string()],
            env: None,
            timeout: None,
        }))
    }

    fn configuration(
        &self,
        _worktree_store: Entity<WorktreeStore>,
        _cx: &AsyncApp,
    ) -> Task<Result<Option<::extension::ContextServerConfiguration>>> {
        Task::ready(Ok(None))
    }
}
