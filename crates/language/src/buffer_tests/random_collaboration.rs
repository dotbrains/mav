use super::*;

#[gpui::test(iterations = 100)]
fn test_random_collaboration(cx: &mut App, mut rng: StdRng) {
    let min_peers = env::var("MIN_PEERS")
        .map(|i| i.parse().expect("invalid `MIN_PEERS` variable"))
        .unwrap_or(1);
    let max_peers = env::var("MAX_PEERS")
        .map(|i| i.parse().expect("invalid `MAX_PEERS` variable"))
        .unwrap_or(5);
    let operations = env::var("OPERATIONS")
        .map(|i| i.parse().expect("invalid `OPERATIONS` variable"))
        .unwrap_or(10);

    let base_text_len = rng.random_range(0..10);
    let base_text = RandomCharIter::new(&mut rng)
        .take(base_text_len)
        .collect::<String>();
    let mut replica_ids = Vec::new();
    let mut buffers = Vec::new();
    let network = Arc::new(Mutex::new(Network::new(rng.clone())));
    let base_buffer = cx.new(|cx| Buffer::local(base_text.as_str(), cx));

    for i in 0..rng.random_range(min_peers..=max_peers) {
        let buffer = cx.new(|cx| {
            let state = base_buffer.read(cx).to_proto(cx);
            let ops = cx
                .foreground_executor()
                .block_on(base_buffer.read(cx).serialize_ops(None, cx));
            let mut buffer =
                Buffer::from_proto(ReplicaId::new(i as u16), Capability::ReadWrite, state, None)
                    .unwrap();
            buffer.apply_ops(
                ops.into_iter()
                    .map(|op| proto::deserialize_operation(op).unwrap()),
                cx,
            );
            buffer.set_group_interval(Duration::from_millis(rng.random_range(0..=200)));
            let network = network.clone();
            cx.subscribe(&cx.entity(), move |buffer, _, event, _| {
                if let BufferEvent::Operation {
                    operation,
                    is_local: true,
                } = event
                {
                    network.lock().broadcast(
                        buffer.replica_id(),
                        vec![proto::serialize_operation(operation)],
                    );
                }
            })
            .detach();
            buffer
        });

        buffers.push(buffer);
        replica_ids.push(ReplicaId::new(i as u16));
        network.lock().add_peer(ReplicaId::new(i as u16));
        log::info!("Adding initial peer with replica id {:?}", replica_ids[i]);
    }

    log::info!("initial text: {:?}", base_text);

    let mut now = Instant::now();
    let mut mutation_count = operations;
    let mut next_diagnostic_id = 0;
    let mut active_selections = BTreeMap::default();
    loop {
        let replica_index = rng.random_range(0..replica_ids.len());
        let replica_id = replica_ids[replica_index];
        let buffer = &mut buffers[replica_index];
        let mut new_buffer = None;
        match rng.random_range(0..100) {
            0..=29 if mutation_count != 0 => {
                buffer.update(cx, |buffer, cx| {
                    buffer.start_transaction_at(now);
                    buffer.randomly_edit(&mut rng, 5, cx);
                    buffer.end_transaction_at(now, cx);
                    log::info!("buffer {:?} text: {:?}", buffer.replica_id(), buffer.text());
                });
                mutation_count -= 1;
            }
            30..=39 if mutation_count != 0 => {
                buffer.update(cx, |buffer, cx| {
                    if rng.random_bool(0.2) {
                        log::info!("peer {:?} clearing active selections", replica_id);
                        active_selections.remove(&replica_id);
                        buffer.remove_active_selections(cx);
                    } else {
                        let mut selections = Vec::new();
                        for id in 0..rng.random_range(1..=5) {
                            let range = buffer.random_byte_range(0, &mut rng);
                            selections.push(Selection {
                                id,
                                start: buffer.anchor_before(range.start),
                                end: buffer.anchor_before(range.end),
                                reversed: false,
                                goal: SelectionGoal::None,
                            });
                        }
                        let selections: Arc<[Selection<Anchor>]> = selections.into();
                        log::info!(
                            "peer {:?} setting active selections: {:?}",
                            replica_id,
                            selections
                        );
                        active_selections.insert(replica_id, selections.clone());
                        buffer.set_active_selections(selections, false, Default::default(), cx);
                    }
                });
                mutation_count -= 1;
            }
            40..=49 if mutation_count != 0 && replica_id == ReplicaId::REMOTE_SERVER => {
                let entry_count = rng.random_range(1..=5);
                buffer.update(cx, |buffer, cx| {
                    let diagnostics = DiagnosticSet::new(
                        (0..entry_count).map(|_| {
                            let range = buffer.random_byte_range(0, &mut rng);
                            let range = range.to_point_utf16(buffer);
                            let range = range.start..range.end;
                            DiagnosticEntry {
                                range,
                                diagnostic: Diagnostic {
                                    message: post_inc(&mut next_diagnostic_id).to_string(),
                                    ..Default::default()
                                },
                            }
                        }),
                        buffer,
                    );
                    log::info!(
                        "peer {:?} setting diagnostics: {:?}",
                        replica_id,
                        diagnostics
                    );
                    buffer.update_diagnostics(LanguageServerId(0), diagnostics, cx);
                });
                mutation_count -= 1;
            }
            50..=59 if replica_ids.len() < max_peers => {
                let old_buffer_state = buffer.read(cx).to_proto(cx);
                let old_buffer_ops = cx
                    .foreground_executor()
                    .block_on(buffer.read(cx).serialize_ops(None, cx));
                let new_replica_id = (0..=replica_ids.len() as u16)
                    .map(ReplicaId::new)
                    .filter(|replica_id| *replica_id != buffer.read(cx).replica_id())
                    .choose(&mut rng)
                    .unwrap();
                log::info!(
                    "Adding new replica {:?} (replicating from {:?})",
                    new_replica_id,
                    replica_id
                );
                new_buffer = Some(cx.new(|cx| {
                    let mut new_buffer = Buffer::from_proto(
                        new_replica_id,
                        Capability::ReadWrite,
                        old_buffer_state,
                        None,
                    )
                    .unwrap();
                    new_buffer.apply_ops(
                        old_buffer_ops
                            .into_iter()
                            .map(|op| deserialize_operation(op).unwrap()),
                        cx,
                    );
                    log::info!(
                        "New replica {:?} text: {:?}",
                        new_buffer.replica_id(),
                        new_buffer.text()
                    );
                    new_buffer.set_group_interval(Duration::from_millis(rng.random_range(0..=200)));
                    let network = network.clone();
                    cx.subscribe(&cx.entity(), move |buffer, _, event, _| {
                        if let BufferEvent::Operation {
                            operation,
                            is_local: true,
                        } = event
                        {
                            network.lock().broadcast(
                                buffer.replica_id(),
                                vec![proto::serialize_operation(operation)],
                            );
                        }
                    })
                    .detach();
                    new_buffer
                }));
                network.lock().replicate(replica_id, new_replica_id);

                if new_replica_id.as_u16() as usize == replica_ids.len() {
                    replica_ids.push(new_replica_id);
                } else {
                    let new_buffer = new_buffer.take().unwrap();
                    while network.lock().has_unreceived(new_replica_id) {
                        let ops = network
                            .lock()
                            .receive(new_replica_id)
                            .into_iter()
                            .map(|op| proto::deserialize_operation(op).unwrap());
                        if ops.len() > 0 {
                            log::info!(
                                "peer {:?} (version: {:?}) applying {} ops from the network. {:?}",
                                new_replica_id,
                                buffer.read(cx).version(),
                                ops.len(),
                                ops
                            );
                            new_buffer.update(cx, |new_buffer, cx| {
                                new_buffer.apply_ops(ops, cx);
                            });
                        }
                    }
                    buffers[new_replica_id.as_u16() as usize] = new_buffer;
                }
            }
            60..=69 if mutation_count != 0 => {
                buffer.update(cx, |buffer, cx| {
                    buffer.randomly_undo_redo(&mut rng, cx);
                    log::info!("buffer {:?} text: {:?}", buffer.replica_id(), buffer.text());
                });
                mutation_count -= 1;
            }
            _ if network.lock().has_unreceived(replica_id) => {
                let ops = network
                    .lock()
                    .receive(replica_id)
                    .into_iter()
                    .map(|op| proto::deserialize_operation(op).unwrap());
                if ops.len() > 0 {
                    log::info!(
                        "peer {:?} (version: {:?}) applying {} ops from the network. {:?}",
                        replica_id,
                        buffer.read(cx).version(),
                        ops.len(),
                        ops
                    );
                    buffer.update(cx, |buffer, cx| buffer.apply_ops(ops, cx));
                }
            }
            _ => {}
        }

        now += Duration::from_millis(rng.random_range(0..=200));
        buffers.extend(new_buffer);

        for buffer in &buffers {
            buffer.read(cx).check_invariants();
        }

        if mutation_count == 0 && network.lock().is_idle() {
            break;
        }
    }

    let first_buffer = buffers[0].read(cx).snapshot();
    for buffer in &buffers[1..] {
        let buffer = buffer.read(cx).snapshot();
        assert_eq!(
            buffer.version(),
            first_buffer.version(),
            "Replica {:?} version != Replica 0 version",
            buffer.replica_id()
        );
        assert_eq!(
            buffer.text(),
            first_buffer.text(),
            "Replica {:?} text != Replica 0 text",
            buffer.replica_id()
        );
        assert_eq!(
            buffer
                .diagnostics_in_range::<_, usize>(0..buffer.len(), false)
                .collect::<Vec<_>>(),
            first_buffer
                .diagnostics_in_range::<_, usize>(0..first_buffer.len(), false)
                .collect::<Vec<_>>(),
            "Replica {:?} diagnostics != Replica 0 diagnostics",
            buffer.replica_id()
        );
    }

    for buffer in &buffers {
        let buffer = buffer.read(cx).snapshot();
        let actual_remote_selections = buffer
            .selections_in_range(Anchor::min_max_range_for_buffer(buffer.remote_id()), false)
            .map(|(replica_id, _, _, selections)| (replica_id, selections.collect::<Vec<_>>()))
            .collect::<Vec<_>>();
        let expected_remote_selections = active_selections
            .iter()
            .filter(|(replica_id, _)| **replica_id != buffer.replica_id())
            .map(|(replica_id, selections)| (*replica_id, selections.iter().collect::<Vec<_>>()))
            .collect::<Vec<_>>();
        assert_eq!(
            actual_remote_selections,
            expected_remote_selections,
            "Replica {:?} remote selections != expected selections",
            buffer.replica_id()
        );
    }
}
