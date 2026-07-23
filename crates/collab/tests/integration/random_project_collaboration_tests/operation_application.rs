use super::*;

pub(super) async fn apply_operation(
    client: &TestClient,
    operation: ClientOperation,
    cx: &mut TestAppContext,
) -> Result<(), TestError> {
    match operation {
        ClientOperation::AcceptIncomingCall => {
            let active_call = cx.read(ActiveCall::global);
            if active_call.read_with(cx, |call, _| call.incoming().borrow().is_none()) {
                Err(TestError::Inapplicable)?;
            }

            log::info!("{}: accepting incoming call", client.username);
            active_call
                .update(cx, |call, cx| call.accept_incoming(cx))
                .await?;
        }

        ClientOperation::RejectIncomingCall => {
            let active_call = cx.read(ActiveCall::global);
            if active_call.read_with(cx, |call, _| call.incoming().borrow().is_none()) {
                Err(TestError::Inapplicable)?;
            }

            log::info!("{}: declining incoming call", client.username);
            active_call.update(cx, |call, cx| call.decline_incoming(cx))?;
        }

        ClientOperation::LeaveCall => {
            let active_call = cx.read(ActiveCall::global);
            if active_call.read_with(cx, |call, _| call.room().is_none()) {
                Err(TestError::Inapplicable)?;
            }

            log::info!("{}: hanging up", client.username);
            active_call.update(cx, |call, cx| call.hang_up(cx)).await?;
        }

        ClientOperation::InviteContactToCall { user_id } => {
            let active_call = cx.read(ActiveCall::global);

            log::info!("{}: inviting {}", client.username, user_id,);
            active_call
                .update(cx, |call, cx| call.invite(user_id.to_proto(), None, cx))
                .await
                .log_err();
        }

        ClientOperation::OpenLocalProject { first_root_name } => {
            log::info!(
                "{}: opening local project at {:?}",
                client.username,
                first_root_name
            );

            let root_path = Path::new(path!("/")).join(&first_root_name);
            client.fs().create_dir(&root_path).await.unwrap();
            client
                .fs()
                .create_file(&root_path.join("main.rs"), Default::default())
                .await
                .unwrap();
            let project = client.build_local_project(root_path, cx).await.0;
            ensure_project_shared(&project, client, cx).await;
            client.local_projects_mut().push(project.clone());
        }

        ClientOperation::AddWorktreeToProject {
            project_root_name,
            new_root_path,
        } => {
            let project = project_for_root_name(client, &project_root_name, cx)
                .ok_or(TestError::Inapplicable)?;

            log::info!(
                "{}: finding/creating local worktree at {:?} to project with root path {}",
                client.username,
                new_root_path,
                project_root_name
            );

            ensure_project_shared(&project, client, cx).await;
            if !client.fs().paths(false).contains(&new_root_path) {
                client.fs().create_dir(&new_root_path).await.unwrap();
            }
            project
                .update(cx, |project, cx| {
                    project.find_or_create_worktree(&new_root_path, true, cx)
                })
                .await
                .unwrap();
        }

        ClientOperation::CloseRemoteProject { project_root_name } => {
            let project = project_for_root_name(client, &project_root_name, cx)
                .ok_or(TestError::Inapplicable)?;

            log::info!(
                "{}: closing remote project with root path {}",
                client.username,
                project_root_name,
            );

            let ix = client
                .dev_server_projects()
                .iter()
                .position(|p| p == &project)
                .unwrap();
            cx.update(|_| {
                client.dev_server_projects_mut().remove(ix);
                client.buffers().retain(|p, _| *p != project);
                drop(project);
            });
        }

        ClientOperation::OpenRemoteProject {
            host_id,
            first_root_name,
        } => {
            let active_call = cx.read(ActiveCall::global);
            let project = active_call
                .update(cx, |call, cx| {
                    let room = call.room().cloned()?;
                    let participant = room
                        .read(cx)
                        .remote_participants()
                        .get(&host_id.to_proto())?;
                    let project_id = participant
                        .projects
                        .iter()
                        .find(|project| project.worktree_root_names[0] == first_root_name)?
                        .id;
                    Some(room.update(cx, |room, cx| {
                        room.join_project(
                            project_id,
                            client.language_registry().clone(),
                            FakeFs::new(cx.background_executor().clone()),
                            cx,
                        )
                    }))
                })
                .ok_or(TestError::Inapplicable)?;

            log::info!(
                "{}: joining remote project of user {}, root name {}",
                client.username,
                host_id,
                first_root_name,
            );

            let project = project.await?;
            client.dev_server_projects_mut().push(project);
        }

        ClientOperation::CreateWorktreeEntry {
            project_root_name,
            is_local,
            full_path,
            is_dir,
        } => {
            let project = project_for_root_name(client, &project_root_name, cx)
                .ok_or(TestError::Inapplicable)?;
            let project_path = project_path_for_full_path(&project, &full_path, cx)
                .ok_or(TestError::Inapplicable)?;

            log::info!(
                "{}: creating {} at path {:?} in {} project {}",
                client.username,
                if is_dir { "dir" } else { "file" },
                full_path,
                if is_local { "local" } else { "remote" },
                project_root_name,
            );

            ensure_project_shared(&project, client, cx).await;
            project
                .update(cx, |p, cx| p.create_entry(project_path, is_dir, cx))
                .await?;
        }

        ClientOperation::OpenBuffer {
            project_root_name,
            is_local,
            full_path,
        } => {
            let project = project_for_root_name(client, &project_root_name, cx)
                .ok_or(TestError::Inapplicable)?;
            let project_path = project_path_for_full_path(&project, &full_path, cx)
                .ok_or(TestError::Inapplicable)?;

            log::info!(
                "{}: opening buffer {:?} in {} project {}",
                client.username,
                full_path,
                if is_local { "local" } else { "remote" },
                project_root_name,
            );

            ensure_project_shared(&project, client, cx).await;
            let buffer = project
                .update(cx, |project, cx| project.open_buffer(project_path, cx))
                .await?;
            client.buffers_for_project(&project).insert(buffer);
        }

        ClientOperation::EditBuffer {
            project_root_name,
            is_local,
            full_path,
            edits,
        } => {
            let project = project_for_root_name(client, &project_root_name, cx)
                .ok_or(TestError::Inapplicable)?;
            let buffer = buffer_for_full_path(client, &project, &full_path, cx)
                .ok_or(TestError::Inapplicable)?;

            log::info!(
                "{}: editing buffer {:?} in {} project {} with {:?}",
                client.username,
                full_path,
                if is_local { "local" } else { "remote" },
                project_root_name,
                edits
            );

            ensure_project_shared(&project, client, cx).await;
            buffer.update(cx, |buffer, cx| {
                let snapshot = buffer.snapshot();
                buffer.edit(
                    edits.into_iter().map(|(range, text)| {
                        let start = snapshot.clip_offset(range.start, Bias::Left);
                        let end = snapshot.clip_offset(range.end, Bias::Right);
                        (start..end, text)
                    }),
                    None,
                    cx,
                );
            });
        }

        ClientOperation::CloseBuffer {
            project_root_name,
            is_local,
            full_path,
        } => {
            let project = project_for_root_name(client, &project_root_name, cx)
                .ok_or(TestError::Inapplicable)?;
            let buffer = buffer_for_full_path(client, &project, &full_path, cx)
                .ok_or(TestError::Inapplicable)?;

            log::info!(
                "{}: closing buffer {:?} in {} project {}",
                client.username,
                full_path,
                if is_local { "local" } else { "remote" },
                project_root_name
            );

            ensure_project_shared(&project, client, cx).await;
            cx.update(|_| {
                client.buffers_for_project(&project).remove(&buffer);
                drop(buffer);
            });
        }

        ClientOperation::SaveBuffer {
            project_root_name,
            is_local,
            full_path,
            detach,
        } => {
            let project = project_for_root_name(client, &project_root_name, cx)
                .ok_or(TestError::Inapplicable)?;
            let buffer = buffer_for_full_path(client, &project, &full_path, cx)
                .ok_or(TestError::Inapplicable)?;

            log::info!(
                "{}: saving buffer {:?} in {} project {}, {}",
                client.username,
                full_path,
                if is_local { "local" } else { "remote" },
                project_root_name,
                if detach { "detaching" } else { "awaiting" }
            );

            ensure_project_shared(&project, client, cx).await;
            let requested_version = buffer.read_with(cx, |buffer, _| buffer.version());
            let save = project.update(cx, |project, cx| project.save_buffer(buffer.clone(), cx));
            let save = cx.spawn(|cx| async move {
                save.await.context("save request failed")?;
                assert!(
                    buffer
                        .read_with(&cx, |buffer, _| { buffer.saved_version().to_owned() })
                        .observed_all(&requested_version)
                );
                anyhow::Ok(())
            });
            if detach {
                cx.update(|cx| save.detach_and_log_err(cx));
            } else {
                save.await?;
            }
        }

        ClientOperation::RequestLspDataInBuffer {
            project_root_name,
            is_local,
            full_path,
            offset,
            kind,
            detach,
        } => {
            let project = project_for_root_name(client, &project_root_name, cx)
                .ok_or(TestError::Inapplicable)?;
            let buffer = buffer_for_full_path(client, &project, &full_path, cx)
                .ok_or(TestError::Inapplicable)?;

            log::info!(
                "{}: request LSP {:?} for buffer {:?} in {} project {}, {}",
                client.username,
                kind,
                full_path,
                if is_local { "local" } else { "remote" },
                project_root_name,
                if detach { "detaching" } else { "awaiting" }
            );

            use futures::{FutureExt as _, TryFutureExt as _};
            let offset = buffer.read_with(cx, |b, _| b.clip_offset(offset, Bias::Left));

            let process_lsp_request = project.update(cx, |project, cx| match kind {
                LspRequestKind::Rename => project
                    .prepare_rename(buffer, offset, cx)
                    .map_ok(|_| ())
                    .boxed(),
                LspRequestKind::Completion => project
                    .completions(&buffer, offset, DEFAULT_COMPLETION_CONTEXT, cx)
                    .map_ok(|_| ())
                    .boxed(),
                LspRequestKind::CodeAction => project
                    .code_actions(&buffer, offset..offset, None, cx)
                    .map(|_| Ok(()))
                    .boxed(),
                LspRequestKind::Definition => project
                    .definitions(&buffer, offset, cx)
                    .map_ok(|_| ())
                    .boxed(),
                LspRequestKind::Highlights => project
                    .document_highlights(&buffer, offset, cx)
                    .map_ok(|_| ())
                    .boxed(),
            });
            let request = cx.foreground_executor().spawn(process_lsp_request);
            if detach {
                request.detach();
            } else {
                request.await?;
            }
        }

        ClientOperation::SearchProject {
            project_root_name,
            is_local,
            query,
            detach,
        } => {
            let project = project_for_root_name(client, &project_root_name, cx)
                .ok_or(TestError::Inapplicable)?;

            log::info!(
                "{}: search {} project {} for {:?}, {}",
                client.username,
                if is_local { "local" } else { "remote" },
                project_root_name,
                query,
                if detach { "detaching" } else { "awaiting" }
            );

            let search = project.update(cx, |project, cx| {
                project.search(
                    SearchQuery::text(
                        query,
                        false,
                        false,
                        false,
                        Default::default(),
                        Default::default(),
                        false,
                        None,
                    )
                    .unwrap(),
                    cx,
                )
            });
            drop(project);
            let search = cx.executor().spawn(async move {
                let mut results = HashMap::default();
                while let Ok(result) = search.rx.recv().await {
                    if let SearchResult::Buffer { buffer, ranges } = result {
                        results.entry(buffer).or_insert(ranges);
                    }
                }
                results
            });
            search.await;
        }

        ClientOperation::WriteFsEntry {
            path,
            is_dir,
            content,
        } => {
            if !client
                .fs()
                .directories(false)
                .contains(&path.parent().unwrap().to_owned())
            {
                return Err(TestError::Inapplicable);
            }

            if is_dir {
                log::info!("{}: creating dir at {:?}", client.username, path);
                client.fs().create_dir(&path).await.unwrap();
            } else {
                let exists = client.fs().metadata(&path).await?.is_some();
                let verb = if exists { "updating" } else { "creating" };
                log::info!("{}: {} file at {:?}", verb, client.username, path);

                client
                    .fs()
                    .save(&path, &content.as_str().into(), text::LineEnding::Unix)
                    .await
                    .unwrap();
            }
        }

        ClientOperation::GitOperation { operation } => {
            git_operations::apply_git_operation(client, operation).await?;
        }
    }
    Ok(())
}
