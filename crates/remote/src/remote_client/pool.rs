use super::*;

pub(super) enum ConnectionPoolEntry {
    Connecting(WeakShared<Task<Result<Arc<dyn RemoteConnection>, Arc<anyhow::Error>>>>),
    Connected(Weak<dyn RemoteConnection>),
}

#[derive(Default)]
pub(super) struct ConnectionPool {
    pub(super) connections: HashMap<RemoteConnectionOptions, ConnectionPoolEntry>,
}

impl Global for ConnectionPool {}

impl ConnectionPool {
    pub(super) fn connect(
        &mut self,
        opts: RemoteConnectionOptions,
        delegate: Arc<dyn RemoteClientDelegate>,
        cx: &mut App,
    ) -> Shared<Task<Result<Arc<dyn RemoteConnection>, Arc<anyhow::Error>>>> {
        let connection = self.connections.get(&opts);
        match connection {
            Some(ConnectionPoolEntry::Connecting(task)) => {
                if let Some(task) = task.upgrade() {
                    log::debug!("Connecting task is still alive");
                    cx.spawn(async move |cx| {
                        delegate.set_status(Some("Waiting for existing connection attempt"), cx)
                    })
                    .detach();
                    return task;
                }
                log::debug!("Connecting task is dead, removing it and restarting a connection");
                self.connections.remove(&opts);
            }
            Some(ConnectionPoolEntry::Connected(remote)) => {
                if let Some(remote) = remote.upgrade()
                    && !remote.has_been_killed()
                {
                    log::debug!("Connection is still alive");
                    return Task::ready(Ok(remote)).shared();
                }
                log::debug!("Connection is dead, removing it and restarting a connection");
                self.connections.remove(&opts);
            }
            None => {
                log::debug!("No existing connection found, starting a new one");
            }
        }

        let task = cx
            .spawn({
                let opts = opts.clone();
                let delegate = delegate.clone();
                async move |cx| {
                    let connection = match opts.clone() {
                        RemoteConnectionOptions::Ssh(opts) => {
                            SshRemoteConnection::new(opts, delegate, cx)
                                .await
                                .map(|connection| Arc::new(connection) as Arc<dyn RemoteConnection>)
                        }
                        RemoteConnectionOptions::Wsl(opts) => {
                            WslRemoteConnection::new(opts, delegate, cx)
                                .await
                                .map(|connection| Arc::new(connection) as Arc<dyn RemoteConnection>)
                        }
                        RemoteConnectionOptions::Docker(opts) => {
                            DockerExecConnection::new(opts, delegate, cx)
                                .await
                                .map(|connection| Arc::new(connection) as Arc<dyn RemoteConnection>)
                        }
                        #[cfg(any(test, feature = "test-support"))]
                        RemoteConnectionOptions::Mock(opts) => match cx.update(|cx| {
                            cx.default_global::<crate::transport::mock::MockConnectionRegistry>()
                                .take(&opts)
                        }) {
                            Some(connection) => Ok(connection.await as Arc<dyn RemoteConnection>),
                            None => Err(anyhow!(
                                "Mock connection not found. Call MockConnection::new() first."
                            )),
                        },
                    };

                    cx.update_global(|pool: &mut Self, _| {
                        debug_assert!(matches!(
                            pool.connections.get(&opts),
                            Some(ConnectionPoolEntry::Connecting(_))
                        ));
                        match connection {
                            Ok(connection) => {
                                pool.connections.insert(
                                    opts.clone(),
                                    ConnectionPoolEntry::Connected(Arc::downgrade(&connection)),
                                );
                                Ok(connection)
                            }
                            Err(error) => {
                                pool.connections.remove(&opts);
                                Err(Arc::new(error))
                            }
                        }
                    })
                }
            })
            .shared();
        if let Some(task) = task.downgrade() {
            self.connections
                .insert(opts.clone(), ConnectionPoolEntry::Connecting(task));
        }
        task
    }
}
