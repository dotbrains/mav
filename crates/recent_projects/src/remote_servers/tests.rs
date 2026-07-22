use super::*;

#[cfg(test)]
mod filter_tests {
    use super::*;

    fn ssh_config_entry(host: &'static str) -> RemoteEntry {
        RemoteEntry::SshConfig {
            host: SharedString::from(host),
        }
    }

    #[test]
    fn test_filter_sync_repopulates_after_rebuild() {
        let entries = vec![ssh_config_entry("alpha"), ssh_config_entry("beta")];
        let mut state = DefaultState {
            filter_data: Arc::new(FilterData::build(&entries)),
            servers: entries,
            filtered_servers: None,
        };

        state.filter_sync("alp");
        let filtered = state.filtered_servers.as_ref().expect("should filter");
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].server_index, 0);
        assert!(!filtered[0].host_positions.is_empty());

        // The filtered index resolves back into the original server list.
        match &state.servers[filtered[0].server_index] {
            RemoteEntry::SshConfig { host, .. } => assert_eq!(host.as_ref(), "alpha"),
            _ => panic!("expected SshConfig"),
        }

        state.filter_sync("");
        assert!(state.filtered_servers.is_none());
    }
}

#[cfg(test)]
mod create_host_tests {
    use super::*;
    use gpui::TestAppContext;

    fn init_test(cx: &mut TestAppContext) -> Arc<AppState> {
        cx.update(|cx| {
            let state = AppState::test(cx);
            crate::init(cx);
            editor::init(cx);
            state
        })
    }

    #[gpui::test]
    async fn test_create_host_from_ssh_config_returns_new_connection_index(
        cx: &mut TestAppContext,
    ) {
        let app_state = init_test(cx);
        let fs: Arc<dyn Fs> = app_state.fs.clone();

        cx.update(|cx| {
            update_settings_file(fs.clone(), cx, |settings, _| {
                settings.remote.ssh_connections = Some(vec![SshConnection {
                    host: "host-a.example".to_string(),
                    projects: BTreeSet::from_iter([RemoteProject {
                        paths: vec!["/path/to/project-a".to_string()],
                    }]),
                    ..Default::default()
                }]);
            });
        });
        cx.run_until_parked();

        let project = Project::test(fs.clone(), [], cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project, window, cx));

        let modal = workspace.update_in(cx, |_workspace, window, cx| {
            let weak = cx.weak_entity();
            cx.new(|cx| RemoteServerProjects::new(false, fs.clone(), window, weak, cx))
        });

        let host_b = SharedString::from("host-b.example");
        let new_index = modal.update(cx, |modal, cx| {
            modal.create_host_from_ssh_config(&host_b, cx)
        });
        cx.run_until_parked();

        let connections = cx.update(|_, cx| {
            RemoteSettings::get_global(cx)
                .ssh_connections()
                .collect::<Vec<_>>()
        });

        assert_eq!(connections.len(), 2);
        assert_eq!(connections[0].host, "host-a.example");
        assert_eq!(connections[1].host, "host-b.example");
        assert_eq!(
            connections[new_index.0].host, "host-b.example",
            "returned index should point at the newly created host"
        );

        assert_eq!(connections[0].projects.len(), 1);
        assert!(connections[new_index.0].projects.is_empty());
    }
}
