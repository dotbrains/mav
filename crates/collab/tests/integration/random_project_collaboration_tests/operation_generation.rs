use super::*;

pub(super) fn generate_operation(
    client: &TestClient,
    rng: &mut StdRng,
    plan: &mut UserTestPlan,
    cx: &TestAppContext,
) -> ClientOperation {
    let call = cx.read(ActiveCall::global);
    loop {
        match rng.random_range(0..100_u32) {
            // Mutate the call
            0..=29 => {
                // Respond to an incoming call
                if call.read_with(cx, |call, _| call.incoming().borrow().is_some()) {
                    break if rng.random_bool(0.7) {
                        ClientOperation::AcceptIncomingCall
                    } else {
                        ClientOperation::RejectIncomingCall
                    };
                }

                match rng.random_range(0..100_u32) {
                    // Invite a contact to the current call
                    0..=70 => {
                        let available_contacts =
                            client.user_store().read_with(cx, |user_store, _| {
                                user_store
                                    .contacts()
                                    .iter()
                                    .filter(|contact| contact.online && !contact.busy)
                                    .cloned()
                                    .collect::<Vec<_>>()
                            });
                        if !available_contacts.is_empty() {
                            let contact = available_contacts.choose(rng).unwrap();
                            break ClientOperation::InviteContactToCall {
                                user_id: UserId(contact.user.legacy_id as i32),
                            };
                        }
                    }

                    // Leave the current call
                    71.. => {
                        if plan.allow_client_disconnection
                            && call.read_with(cx, |call, _| call.room().is_some())
                        {
                            break ClientOperation::LeaveCall;
                        }
                    }
                }
            }

            // Mutate projects
            30..=59 => match rng.random_range(0..100_u32) {
                // Open a new project
                0..=70 => {
                    // Open a remote project
                    if let Some(room) = call.read_with(cx, |call, _| call.room().cloned()) {
                        let existing_dev_server_project_ids = cx.read(|cx| {
                            client
                                .dev_server_projects()
                                .iter()
                                .map(|p| p.read(cx).remote_id().unwrap())
                                .collect::<Vec<_>>()
                        });
                        let new_dev_server_projects = room.read_with(cx, |room, _| {
                            room.remote_participants()
                                .values()
                                .flat_map(|participant| {
                                    participant.projects.iter().filter_map(|project| {
                                        if existing_dev_server_project_ids.contains(&project.id) {
                                            None
                                        } else {
                                            Some((
                                                UserId::from_proto(participant.user.legacy_id),
                                                project.worktree_root_names[0].clone(),
                                            ))
                                        }
                                    })
                                })
                                .collect::<Vec<_>>()
                        });
                        if !new_dev_server_projects.is_empty() {
                            let (host_id, first_root_name) =
                                new_dev_server_projects.choose(rng).unwrap().clone();
                            break ClientOperation::OpenRemoteProject {
                                host_id,
                                first_root_name,
                            };
                        }
                    }
                    // Open a local project
                    else {
                        let first_root_name = plan.next_root_dir_name();
                        break ClientOperation::OpenLocalProject { first_root_name };
                    }
                }

                // Close a remote project
                71..=80 => {
                    if !client.dev_server_projects().is_empty() {
                        let project = client.dev_server_projects().choose(rng).unwrap().clone();
                        let first_root_name = root_name_for_project(&project, cx);
                        break ClientOperation::CloseRemoteProject {
                            project_root_name: first_root_name,
                        };
                    }
                }

                // Mutate project worktrees
                81.. => match rng.random_range(0..100_u32) {
                    // Add a worktree to a local project
                    0..=50 => {
                        let Some(project) = client.local_projects().choose(rng).cloned() else {
                            continue;
                        };
                        let project_root_name = root_name_for_project(&project, cx);
                        let mut paths = client.fs().paths(false);
                        paths.remove(0);
                        let new_root_path = if paths.is_empty() || rng.random() {
                            Path::new(path!("/")).join(plan.next_root_dir_name())
                        } else {
                            paths.choose(rng).unwrap().clone()
                        };
                        break ClientOperation::AddWorktreeToProject {
                            project_root_name,
                            new_root_path,
                        };
                    }

                    // Add an entry to a worktree
                    _ => {
                        let Some(project) = choose_random_project(client, rng) else {
                            continue;
                        };
                        let project_root_name = root_name_for_project(&project, cx);
                        let is_local = project.read_with(cx, |project, _| project.is_local());
                        let worktree = project.read_with(cx, |project, cx| {
                            project
                                .worktrees(cx)
                                .filter(|worktree| {
                                    let worktree = worktree.read(cx);
                                    worktree.is_visible()
                                        && worktree.entries(false, 0).any(|e| e.is_file())
                                        && worktree.root_entry().is_some_and(|e| e.is_dir())
                                })
                                .choose(rng)
                        });
                        let Some(worktree) = worktree else { continue };
                        let is_dir = rng.random::<bool>();
                        let mut full_path =
                            worktree.read_with(cx, |w, _| w.root_name().to_rel_path_buf());
                        full_path.push(rel_path(&gen_file_name(rng)));
                        if !is_dir {
                            full_path.set_extension("rs");
                        }
                        break ClientOperation::CreateWorktreeEntry {
                            project_root_name,
                            is_local,
                            full_path,
                            is_dir,
                        };
                    }
                },
            },

            // Query and mutate buffers
            60..=90 => {
                let Some(project) = choose_random_project(client, rng) else {
                    continue;
                };
                let project_root_name = root_name_for_project(&project, cx);
                let is_local = project.read_with(cx, |project, _| project.is_local());

                match rng.random_range(0..100_u32) {
                    // Manipulate an existing buffer
                    0..=70 => {
                        let Some(buffer) = client
                            .buffers_for_project(&project)
                            .iter()
                            .choose(rng)
                            .cloned()
                        else {
                            continue;
                        };

                        let full_path = buffer.read_with(cx, |buffer, cx| {
                            let file = buffer.file().unwrap();
                            let worktree = project
                                .read(cx)
                                .worktree_for_id(file.worktree_id(cx), cx)
                                .unwrap();
                            worktree
                                .read(cx)
                                .root_name()
                                .join(file.path())
                                .to_rel_path_buf()
                        });

                        match rng.random_range(0..100_u32) {
                            // Close the buffer
                            0..=15 => {
                                break ClientOperation::CloseBuffer {
                                    project_root_name,
                                    is_local,
                                    full_path,
                                };
                            }
                            // Save the buffer
                            16..=29 if buffer.read_with(cx, |b, _| b.is_dirty()) => {
                                let detach = rng.random_bool(0.3);
                                break ClientOperation::SaveBuffer {
                                    project_root_name,
                                    is_local,
                                    full_path,
                                    detach,
                                };
                            }
                            // Edit the buffer
                            30..=69 => {
                                let edits = buffer
                                    .read_with(cx, |buffer, _| buffer.get_random_edits(rng, 3));
                                break ClientOperation::EditBuffer {
                                    project_root_name,
                                    is_local,
                                    full_path,
                                    edits,
                                };
                            }
                            // Make an LSP request
                            _ => {
                                let offset = buffer.read_with(cx, |buffer, _| {
                                    buffer.clip_offset(
                                        rng.random_range(0..=buffer.len()),
                                        language::Bias::Left,
                                    )
                                });
                                let detach = rng.random();
                                break ClientOperation::RequestLspDataInBuffer {
                                    project_root_name,
                                    full_path,
                                    offset,
                                    is_local,
                                    kind: match rng.random_range(0..5_u32) {
                                        0 => LspRequestKind::Rename,
                                        1 => LspRequestKind::Highlights,
                                        2 => LspRequestKind::Definition,
                                        3 => LspRequestKind::CodeAction,
                                        4.. => LspRequestKind::Completion,
                                    },
                                    detach,
                                };
                            }
                        }
                    }

                    71..=80 => {
                        let query = rng.random_range('a'..='z').to_string();
                        let detach = rng.random_bool(0.3);
                        break ClientOperation::SearchProject {
                            project_root_name,
                            is_local,
                            query,
                            detach,
                        };
                    }

                    // Open a buffer
                    81.. => {
                        let worktree = project.read_with(cx, |project, cx| {
                            project
                                .worktrees(cx)
                                .filter(|worktree| {
                                    let worktree = worktree.read(cx);
                                    worktree.is_visible()
                                        && worktree.entries(false, 0).any(|e| e.is_file())
                                })
                                .choose(rng)
                        });
                        let Some(worktree) = worktree else { continue };
                        let full_path = worktree.read_with(cx, |worktree, _| {
                            let entry = worktree
                                .entries(false, 0)
                                .filter(|e| e.is_file())
                                .choose(rng)
                                .unwrap();
                            if entry.path.as_ref().is_empty() {
                                worktree.root_name().into()
                            } else {
                                worktree.root_name().join(&entry.path)
                            }
                        });
                        break ClientOperation::OpenBuffer {
                            project_root_name,
                            is_local,
                            full_path: full_path.to_rel_path_buf(),
                        };
                    }
                }
            }

            // Update a git related action
            91..=95 => {
                break ClientOperation::GitOperation {
                    operation: generate_git_operation(rng, client),
                };
            }

            // Create or update a file or directory
            96.. => {
                let is_dir = rng.random::<bool>();
                let content;
                let mut path;
                let dir_paths = client.fs().directories(false);

                if is_dir {
                    content = String::new();
                    path = dir_paths.choose(rng).unwrap().clone();
                    path.push(gen_file_name(rng));
                } else {
                    content = distr::Alphanumeric.sample_string(rng, 16);

                    // Create a new file or overwrite an existing file
                    let file_paths = client.fs().files();
                    if file_paths.is_empty() || rng.random_bool(0.5) {
                        path = dir_paths.choose(rng).unwrap().clone();
                        path.push(gen_file_name(rng));
                        path.set_extension("rs");
                    } else {
                        path = file_paths.choose(rng).unwrap().clone()
                    };
                }
                break ClientOperation::WriteFsEntry {
                    path,
                    is_dir,
                    content,
                };
            }
        }
    }
}
