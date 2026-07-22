use super::*;

impl CollabPanel {
    fn update_entries(&mut self, select_same_item: bool, cx: &mut Context<Self>) {
        let query = self.filter_editor.read(cx).text(cx);
        let fg_executor = cx.foreground_executor().clone();
        let executor = cx.background_executor().clone();

        let prev_selected_entry = self.selection.and_then(|ix| self.entries.get(ix).cloned());
        let old_entries = mem::take(&mut self.entries);
        let mut scroll_to_top = false;

        if let Some(room) = ActiveCall::global(cx).read(cx).room() {
            self.entries.push(ListEntry::Header(Section::ActiveCall));
            if !old_entries
                .iter()
                .any(|entry| matches!(entry, ListEntry::Header(Section::ActiveCall)))
            {
                scroll_to_top = true;
            }

            if !self.collapsed_sections.contains(&Section::ActiveCall) {
                let room = room.read(cx);

                if query.is_empty()
                    && let Some(channel_id) = room.channel_id()
                {
                    self.entries.push(ListEntry::ChannelNotes { channel_id });
                }

                // Populate the active user.
                if let Some(user) = self.user_store.read(cx).current_user() {
                    self.match_candidates.clear();
                    self.match_candidates
                        .push(StringMatchCandidate::new(0, &user.username));
                    let matches = fg_executor.block_on(match_strings(
                        &self.match_candidates,
                        &query,
                        true,
                        true,
                        usize::MAX,
                        &Default::default(),
                        executor.clone(),
                    ));
                    if !matches.is_empty() {
                        let user_id = user.legacy_id;
                        self.entries.push(ListEntry::CallParticipant {
                            user,
                            peer_id: None,
                            is_pending: false,
                            role: room.local_participant().role,
                        });
                        let mut projects = room.local_participant().projects.iter().peekable();
                        while let Some(project) = projects.next() {
                            self.entries.push(ListEntry::ParticipantProject {
                                project_id: project.id,
                                worktree_root_names: project.worktree_root_names.clone(),
                                host_user_id: user_id,
                                is_last: projects.peek().is_none() && !room.is_sharing_screen(),
                            });
                        }
                        if room.is_sharing_screen() {
                            self.entries.push(ListEntry::ParticipantScreen {
                                peer_id: None,
                                is_last: true,
                            });
                        }
                    }
                }

                // Populate remote participants.
                self.match_candidates.clear();
                self.match_candidates
                    .extend(room.remote_participants().values().map(|participant| {
                        StringMatchCandidate::new(
                            participant.user.legacy_id as usize,
                            &participant.user.username,
                        )
                    }));
                let mut matches = fg_executor.block_on(match_strings(
                    &self.match_candidates,
                    &query,
                    true,
                    true,
                    usize::MAX,
                    &Default::default(),
                    executor.clone(),
                ));
                matches.sort_by(|a, b| {
                    let a_is_guest = room.role_for_user(a.candidate_id as u64)
                        == Some(proto::ChannelRole::Guest);
                    let b_is_guest = room.role_for_user(b.candidate_id as u64)
                        == Some(proto::ChannelRole::Guest);
                    a_is_guest
                        .cmp(&b_is_guest)
                        .then_with(|| a.string.cmp(&b.string))
                });
                for mat in matches {
                    let user_id = mat.candidate_id as u64;
                    let participant = &room.remote_participants()[&user_id];
                    self.entries.push(ListEntry::CallParticipant {
                        user: participant.user.clone(),
                        peer_id: Some(participant.peer_id),
                        is_pending: false,
                        role: participant.role,
                    });
                    let mut projects = participant.projects.iter().peekable();
                    while let Some(project) = projects.next() {
                        self.entries.push(ListEntry::ParticipantProject {
                            project_id: project.id,
                            worktree_root_names: project.worktree_root_names.clone(),
                            host_user_id: participant.user.legacy_id,
                            is_last: projects.peek().is_none() && !participant.has_video_tracks(),
                        });
                    }
                    if participant.has_video_tracks() {
                        self.entries.push(ListEntry::ParticipantScreen {
                            peer_id: Some(participant.peer_id),
                            is_last: true,
                        });
                    }
                }

                // Populate pending participants.
                self.match_candidates.clear();
                self.match_candidates.extend(
                    room.pending_participants()
                        .iter()
                        .enumerate()
                        .map(|(id, participant)| {
                            StringMatchCandidate::new(id, &participant.username)
                        }),
                );
                let matches = fg_executor.block_on(match_strings(
                    &self.match_candidates,
                    &query,
                    true,
                    true,
                    usize::MAX,
                    &Default::default(),
                    executor.clone(),
                ));
                self.entries
                    .extend(matches.iter().map(|mat| ListEntry::CallParticipant {
                        user: room.pending_participants()[mat.candidate_id].clone(),
                        peer_id: None,
                        is_pending: true,
                        role: proto::ChannelRole::Member,
                    }));
            }
        }

        let mut request_entries = Vec::new();

        let channel_store = self.channel_store.read(cx);
        let user_store = self.user_store.read(cx);

        let favorite_ids = channel_store.favorite_channel_ids();
        if !favorite_ids.is_empty() {
            let favorite_channels: Vec<_> = favorite_ids
                .iter()
                .filter_map(|id| channel_store.channel_for_id(*id))
                .collect();

            self.match_candidates.clear();
            self.match_candidates.extend(
                favorite_channels
                    .iter()
                    .enumerate()
                    .map(|(ix, channel)| StringMatchCandidate::new(ix, &channel.name)),
            );

            let matches = fg_executor.block_on(match_strings(
                &self.match_candidates,
                &query,
                true,
                true,
                usize::MAX,
                &Default::default(),
                executor.clone(),
            ));

            if !matches.is_empty() || query.is_empty() {
                self.entries
                    .push(ListEntry::Header(Section::FavoriteChannels));

                let matches_by_candidate: HashMap<usize, &StringMatch> =
                    matches.iter().map(|mat| (mat.candidate_id, mat)).collect();

                for (ix, channel) in favorite_channels.iter().enumerate() {
                    if !query.is_empty() && !matches_by_candidate.contains_key(&ix) {
                        continue;
                    }
                    self.entries.push(ListEntry::Channel {
                        channel: (*channel).clone(),
                        depth: 0,
                        has_children: false,
                        is_favorite: true,
                        string_match: matches_by_candidate.get(&ix).cloned().cloned(),
                    });
                }
            }
        }

        self.entries.push(ListEntry::Header(Section::Channels));

        if channel_store.channel_count() > 0 || self.channel_editing_state.is_some() {
            self.match_candidates.clear();
            self.match_candidates.extend(
                channel_store
                    .ordered_channels()
                    .enumerate()
                    .map(|(ix, (_, channel))| StringMatchCandidate::new(ix, &channel.name)),
            );
            let mut channels = channel_store
                .ordered_channels()
                .map(|(_, chan)| chan)
                .collect::<Vec<_>>();
            let matches = fg_executor.block_on(match_strings(
                &self.match_candidates,
                &query,
                true,
                true,
                usize::MAX,
                &Default::default(),
                executor.clone(),
            ));

            let matches_by_id: HashMap<_, _> = matches
                .iter()
                .map(|mat| (channels[mat.candidate_id].id, mat.clone()))
                .collect();

            let channel_ids_of_matches_or_parents: HashSet<_> = matches
                .iter()
                .flat_map(|mat| {
                    let match_channel = channels[mat.candidate_id];

                    match_channel
                        .parent_path
                        .iter()
                        .copied()
                        .chain(Some(match_channel.id))
                })
                .collect();

            channels.retain(|chan| channel_ids_of_matches_or_parents.contains(&chan.id));

            if self.filter_occupied_channels {
                let occupied_channel_ids_or_ancestors: HashSet<_> = channel_store
                    .ordered_channels()
                    .map(|(_, channel)| channel)
                    .filter(|channel| !channel_store.channel_participants(channel.id).is_empty())
                    .flat_map(|channel| channel.parent_path.iter().copied().chain(Some(channel.id)))
                    .collect();
                channels.retain(|channel| occupied_channel_ids_or_ancestors.contains(&channel.id));
            }

            if let Some(state) = &self.channel_editing_state
                && matches!(state, ChannelEditingState::Create { location: None, .. })
            {
                self.entries.push(ListEntry::ChannelEditor { depth: 0 });
            }

            let should_respect_collapse = query.is_empty() && !self.filter_occupied_channels;
            let mut collapse_depth = None;

            for (idx, channel) in channels.into_iter().enumerate() {
                let depth = channel.parent_path.len();

                if should_respect_collapse {
                    if collapse_depth.is_none() && self.is_channel_collapsed(channel.id) {
                        collapse_depth = Some(depth);
                    } else if let Some(collapsed_depth) = collapse_depth {
                        if depth > collapsed_depth {
                            continue;
                        }
                        if self.is_channel_collapsed(channel.id) {
                            collapse_depth = Some(depth);
                        } else {
                            collapse_depth = None;
                        }
                    }
                }

                let has_children = channel_store
                    .channel_at_index(idx + 1)
                    .is_some_and(|next_channel| next_channel.parent_path.ends_with(&[channel.id]));

                match &self.channel_editing_state {
                    Some(ChannelEditingState::Create {
                        location: parent_id,
                        ..
                    }) if *parent_id == Some(channel.id) => {
                        self.entries.push(ListEntry::Channel {
                            channel: channel.clone(),
                            depth,
                            has_children: false,
                            is_favorite: false,
                            string_match: matches_by_id.get(&channel.id).map(|mat| (*mat).clone()),
                        });
                        self.entries
                            .push(ListEntry::ChannelEditor { depth: depth + 1 });
                    }
                    Some(ChannelEditingState::Rename {
                        location: parent_id,
                        ..
                    }) if parent_id == &channel.id => {
                        self.entries.push(ListEntry::ChannelEditor { depth });
                    }
                    _ => {
                        self.entries.push(ListEntry::Channel {
                            channel: channel.clone(),
                            depth,
                            has_children,
                            is_favorite: false,
                            string_match: matches_by_id.get(&channel.id).map(|mat| (*mat).clone()),
                        });
                    }
                }
            }
        }

        let channel_invites = channel_store.channel_invitations();
        if !channel_invites.is_empty() {
            self.match_candidates.clear();
            self.match_candidates.extend(
                channel_invites
                    .iter()
                    .enumerate()
                    .map(|(ix, channel)| StringMatchCandidate::new(ix, &channel.name)),
            );
            let matches = fg_executor.block_on(match_strings(
                &self.match_candidates,
                &query,
                true,
                true,
                usize::MAX,
                &Default::default(),
                executor.clone(),
            ));
            request_entries.extend(
                matches
                    .iter()
                    .map(|mat| ListEntry::ChannelInvite(channel_invites[mat.candidate_id].clone())),
            );

            if !request_entries.is_empty() {
                self.entries
                    .push(ListEntry::Header(Section::ChannelInvites));
                if !self.collapsed_sections.contains(&Section::ChannelInvites) {
                    self.entries.append(&mut request_entries);
                }
            }
        }

        self.entries.push(ListEntry::Header(Section::Contacts));

        request_entries.clear();
        let incoming = user_store.incoming_contact_requests();
        if !incoming.is_empty() {
            self.match_candidates.clear();
            self.match_candidates.extend(
                incoming
                    .iter()
                    .enumerate()
                    .map(|(ix, user)| StringMatchCandidate::new(ix, &user.username)),
            );
            let matches = fg_executor.block_on(match_strings(
                &self.match_candidates,
                &query,
                true,
                true,
                usize::MAX,
                &Default::default(),
                executor.clone(),
            ));
            request_entries.extend(
                matches
                    .iter()
                    .map(|mat| ListEntry::IncomingRequest(incoming[mat.candidate_id].clone())),
            );
        }

        let outgoing = user_store.outgoing_contact_requests();
        if !outgoing.is_empty() {
            self.match_candidates.clear();
            self.match_candidates.extend(
                outgoing
                    .iter()
                    .enumerate()
                    .map(|(ix, user)| StringMatchCandidate::new(ix, &user.username)),
            );
            let matches = fg_executor.block_on(match_strings(
                &self.match_candidates,
                &query,
                true,
                true,
                usize::MAX,
                &Default::default(),
                executor.clone(),
            ));
            request_entries.extend(
                matches
                    .iter()
                    .map(|mat| ListEntry::OutgoingRequest(outgoing[mat.candidate_id].clone())),
            );
        }

        if !request_entries.is_empty() {
            self.entries
                .push(ListEntry::Header(Section::ContactRequests));
            if !self.collapsed_sections.contains(&Section::ContactRequests) {
                self.entries.append(&mut request_entries);
            }
        }

        let contacts = user_store.contacts();
        if !contacts.is_empty() {
            self.match_candidates.clear();
            self.match_candidates.extend(
                contacts
                    .iter()
                    .enumerate()
                    .map(|(ix, contact)| StringMatchCandidate::new(ix, &contact.user.username)),
            );

            let matches = fg_executor.block_on(match_strings(
                &self.match_candidates,
                &query,
                true,
                true,
                usize::MAX,
                &Default::default(),
                executor,
            ));

            let (online_contacts, offline_contacts) = matches
                .iter()
                .partition::<Vec<_>, _>(|mat| contacts[mat.candidate_id].online);

            for (matches, section) in [
                (online_contacts, Section::Online),
                (offline_contacts, Section::Offline),
            ] {
                if !matches.is_empty() {
                    self.entries.push(ListEntry::Header(section));
                    if !self.collapsed_sections.contains(&section) {
                        let active_call = &ActiveCall::global(cx).read(cx);
                        for mat in matches {
                            let contact = &contacts[mat.candidate_id];
                            self.entries.push(ListEntry::Contact {
                                contact: contact.clone(),
                                calling: active_call
                                    .pending_invites()
                                    .contains(&contact.user.legacy_id),
                            });
                        }
                    }
                }
            }
        }

        if incoming.is_empty() && outgoing.is_empty() && contacts.is_empty() {
            self.entries.push(ListEntry::ContactPlaceholder);
        }

        self.restore_selection_and_scroll(
            select_same_item,
            prev_selected_entry,
            old_entries,
            scroll_to_top,
        );

        cx.notify();
    }
}
