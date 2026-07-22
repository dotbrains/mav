use super::*;

#[gpui::test]
async fn test_remote_agent_fs_tool_calls(cx: &mut TestAppContext, server_cx: &mut TestAppContext) {
    let fs = FakeFs::new(server_cx.executor());
    fs.insert_tree(
        path!("/project"),
        json!({
            "a.txt": "A",
            "b.txt": "B",
        }),
    )
    .await;

    let (project, _headless_project) = init_test(&fs, cx, server_cx).await;
    project
        .update(cx, |project, cx| {
            project.find_or_create_worktree(path!("/project"), true, cx)
        })
        .await
        .unwrap();

    let action_log = cx.new(|_| action_log::ActionLog::new(project.clone()));

    let input = ReadFileToolInput {
        path: "project/b.txt".into(),
        start_line: None,
        end_line: None,
    };
    let read_tool = Arc::new(ReadFileTool::new(project, action_log, true));
    let (event_stream, _) = ToolCallEventStream::test();

    let exists_result = cx.update(|cx| {
        read_tool
            .clone()
            .run(ToolInput::resolved(input), event_stream.clone(), cx)
    });
    let output = exists_result.await.unwrap();
    assert_eq!(
        output,
        LanguageModelToolResultContent::Text("     1\tB".into())
    );

    let input = ReadFileToolInput {
        path: "project/c.txt".into(),
        start_line: None,
        end_line: None,
    };
    let does_not_exist_result =
        cx.update(|cx| read_tool.run(ToolInput::resolved(input), event_stream, cx));
    does_not_exist_result.await.unwrap_err();
}

#[gpui::test]
async fn test_adding_remote_skill(cx: &mut TestAppContext, server_cx: &mut TestAppContext) {
    use acp_thread::AgentConnection as _;

    let fs = FakeFs::new(server_cx.executor());
    fs.insert_tree(
        path!("/project"),
        json!({
            ".agents": {
                "skills": {
                    "test-skill": {
                        "SKILL.md": "---\nname: test-skill\ndescription: test description\n---\ntest body"
                    }
                }
            }
        }),
    )
    .await;

    let (project, _headless_project) = init_test(&fs, cx, server_cx).await;
    cx.update(|cx| {
        LanguageModelRegistry::test(cx);
    });
    let (_worktree, _rel_path) = project
        .update(cx, |project, cx| {
            project.find_or_create_worktree(path!("/project"), true, cx)
        })
        .await
        .unwrap();
    cx.run_until_parked();
    let thread_store = cx.new(|cx| ThreadStore::new(cx));
    let agent = cx.update(|cx| NativeAgent::new(thread_store, Templates::new(), fs.clone(), cx));
    let connection = Rc::new(NativeAgentConnection(agent.clone()));
    let _acp_thread = cx
        .update(|cx| {
            connection.clone().new_session(
                project.clone(),
                PathList::new(&[Path::new("/project")]),
                cx,
            )
        })
        .await
        .unwrap();
    cx.run_until_parked();

    let skill_tool = Arc::new(SkillTool::with_body_resolver(
        skills_resolver_for_project(agent.downgrade(), project.entity_id()),
        skill_body_resolver_for_project(project.clone(), fs.clone()),
    ));
    let (event_stream, mut event_stream_rx) = ToolCallEventStream::test();

    let input = SkillToolInput {
        name: "test-skill".into(),
    };
    let task = cx.update(|cx| {
        skill_tool
            .clone()
            .run(ToolInput::resolved(input), event_stream.clone(), cx)
    });

    // The project-local skill is not a built-in, so the tool requests
    // authorization. Approve it so the tool can proceed.
    let authorization = event_stream_rx.expect_authorization().await;
    authorization
        .response
        .send(acp_thread::SelectedPermissionOutcome::new(
            agent_client_protocol::schema::v1::PermissionOptionId::new("allow"),
            agent_client_protocol::schema::v1::PermissionOptionKind::AllowOnce,
        ))
        .unwrap();

    let output = task.await.unwrap();
    cx.run_until_parked();
    let expected = format!(
        concat!(
            "<skill_content name=\"test-skill\">\n",
            "<source>project-local</source>\n",
            "<worktree>project</worktree>\n",
            "<directory>{}</directory>\n",
            "Relative paths in this skill resolve against <directory>.\n",
            "\n",
            "test body\n",
            "</skill_content>\n",
        ),
        path!("/project/.agents/skills/test-skill"),
    );
    assert_eq!(output, SkillToolOutput::Found { rendered: expected });

    fs.create_dir(Path::new(path!("/project/.agents/skills/test-2")))
        .await
        .unwrap();
    fs.insert_file(
        path!("/project/.agents/skills/test-2/SKILL.md"),
        "---\nname: test-2\ndescription: test description\n---\ntest body"
            .as_bytes()
            .into(),
    )
    .await;

    cx.run_until_parked();
    cx.update(|cx| connection.refresh_skills_for_project(project, cx));
    cx.run_until_parked();

    let input2 = SkillToolInput {
        name: "test-2".into(),
    };
    let task = cx.update(|cx| {
        skill_tool
            .clone()
            .run(ToolInput::resolved(input2), event_stream.clone(), cx)
    });

    let authorization = event_stream_rx.expect_authorization().await;
    authorization
        .response
        .send(acp_thread::SelectedPermissionOutcome::new(
            agent_client_protocol::schema::v1::PermissionOptionId::new("allow"),
            agent_client_protocol::schema::v1::PermissionOptionKind::AllowOnce,
        ))
        .unwrap();

    let output = task.await.unwrap();
    let expected2 = format!(
        concat!(
            "<skill_content name=\"test-2\">\n",
            "<source>project-local</source>\n",
            "<worktree>project</worktree>\n",
            "<directory>{}</directory>\n",
            "Relative paths in this skill resolve against <directory>.\n",
            "\n",
            "test body\n",
            "</skill_content>\n",
        ),
        path!("/project/.agents/skills/test-2"),
    );
    assert_eq!(
        output,
        SkillToolOutput::Found {
            rendered: expected2
        }
    );
}

#[gpui::test]
async fn test_remote_external_agent_server(
    cx: &mut TestAppContext,
    server_cx: &mut TestAppContext,
) {
    let fs = FakeFs::new(server_cx.executor());
    fs.insert_tree(path!("/project"), json!({})).await;

    let (project, _headless_project) = init_test(&fs, cx, server_cx).await;
    project
        .update(cx, |project, cx| {
            project.find_or_create_worktree(path!("/project"), true, cx)
        })
        .await
        .unwrap();
    let names = project.update(cx, |project, cx| {
        project
            .agent_server_store()
            .read(cx)
            .external_agents()
            .map(|name| name.to_string())
            .collect::<Vec<_>>()
    });
    pretty_assertions::assert_eq!(names, Vec::<String>::new());
    server_cx.update_global::<SettingsStore, _>(|settings_store, cx| {
        settings_store
            .set_server_settings(
                &json!({
                    "agent_servers": {
                        "foo": {
                            "type": "custom",
                            "command": "foo-cli",
                            "args": ["--flag"],
                            "env": {
                                "VAR": "val"
                            }
                        }
                    }
                })
                .to_string(),
                cx,
            )
            .unwrap();
    });
    server_cx.run_until_parked();
    cx.run_until_parked();
    let names = project.update(cx, |project, cx| {
        project
            .agent_server_store()
            .read(cx)
            .external_agents()
            .map(|name| name.to_string())
            .collect::<Vec<_>>()
    });
    pretty_assertions::assert_eq!(names, ["foo"]);
    let command = project
        .update(cx, |project, cx| {
            project.agent_server_store().update(cx, |store, cx| {
                store
                    .get_external_agent(&"foo".into())
                    .unwrap()
                    .get_command(
                        vec![],
                        HashMap::from_iter([("OTHER_VAR".into(), "other-val".into())]),
                        &mut cx.to_async(),
                    )
            })
        })
        .await
        .unwrap();
    assert_eq!(
        command,
        AgentServerCommand {
            path: "foo-cli".into(),
            args: vec!["--flag".into()],
            env: Some(HashMap::from_iter([
                ("NO_BROWSER".into(), "1".into()),
                ("VAR".into(), "val".into()),
                ("OTHER_VAR".into(), "other-val".into())
            ]))
        }
    );
}
