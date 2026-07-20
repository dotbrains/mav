use super::*;

pub(crate) fn setup_context_server(
    name: &'static str,
    tools: Vec<context_server::types::Tool>,
    context_server_store: &Entity<ContextServerStore>,
    cx: &mut TestAppContext,
) -> mpsc::UnboundedReceiver<(
    context_server::types::CallToolParams,
    oneshot::Sender<context_server::types::CallToolResponse>,
)> {
    cx.update(|cx| {
        let mut settings = ProjectSettings::get_global(cx).clone();
        settings.context_servers.insert(
            name.into(),
            project::project_settings::ContextServerSettings::Stdio {
                enabled: true,
                remote: false,
                command: ContextServerCommand {
                    path: "somebinary".into(),
                    args: Vec::new(),
                    env: None,
                    timeout: None,
                },
            },
        );
        ProjectSettings::override_global(settings, cx);
    });

    let (mcp_tool_calls_tx, mcp_tool_calls_rx) = mpsc::unbounded();
    let fake_transport = context_server::test::create_fake_transport(name, cx.executor())
        .on_request::<context_server::types::requests::Initialize, _>(move |_params| async move {
            context_server::types::InitializeResponse {
                protocol_version: context_server::types::ProtocolVersion(
                    context_server::types::LATEST_PROTOCOL_VERSION.to_string(),
                ),
                server_info: context_server::types::Implementation {
                    name: name.into(),
                    title: None,
                    version: "1.0.0".to_string(),
                    description: None,
                },
                capabilities: context_server::types::ServerCapabilities {
                    tools: Some(context_server::types::ToolsCapabilities {
                        list_changed: Some(true),
                    }),
                    ..Default::default()
                },
                meta: None,
            }
        })
        .on_request::<context_server::types::requests::ListTools, _>(move |_params| {
            let tools = tools.clone();
            async move {
                context_server::types::ListToolsResponse {
                    tools,
                    next_cursor: None,
                    meta: None,
                }
            }
        })
        .on_request::<context_server::types::requests::CallTool, _>(move |params| {
            let mcp_tool_calls_tx = mcp_tool_calls_tx.clone();
            async move {
                let (response_tx, response_rx) = oneshot::channel();
                mcp_tool_calls_tx
                    .unbounded_send((params, response_tx))
                    .unwrap();
                response_rx.await.unwrap()
            }
        });
    context_server_store.update(cx, |store, cx| {
        store.start_server(
            Arc::new(ContextServer::new(
                ContextServerId(name.into()),
                Arc::new(fake_transport),
            )),
            cx,
        );
    });
    cx.run_until_parked();
    mcp_tool_calls_rx
}
