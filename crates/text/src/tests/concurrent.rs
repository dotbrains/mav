use super::*;

#[test]
fn test_concurrent_edits() {
    let text = "abcdef";

    let mut buffer1 = Buffer::new(ReplicaId::new(1), BufferId::new(1).unwrap(), text);
    let mut buffer2 = Buffer::new(ReplicaId::new(2), BufferId::new(1).unwrap(), text);
    let mut buffer3 = Buffer::new(ReplicaId::new(3), BufferId::new(1).unwrap(), text);

    let buf1_op = buffer1.edit([(1..2, "12")]);
    assert_eq!(buffer1.text(), "a12cdef");
    let buf2_op = buffer2.edit([(3..4, "34")]);
    assert_eq!(buffer2.text(), "abc34ef");
    let buf3_op = buffer3.edit([(5..6, "56")]);
    assert_eq!(buffer3.text(), "abcde56");

    buffer1.apply_op(buf2_op.clone());
    buffer1.apply_op(buf3_op.clone());
    buffer2.apply_op(buf1_op.clone());
    buffer2.apply_op(buf3_op);
    buffer3.apply_op(buf1_op);
    buffer3.apply_op(buf2_op);

    assert_eq!(buffer1.text(), "a12c34e56");
    assert_eq!(buffer2.text(), "a12c34e56");
    assert_eq!(buffer3.text(), "a12c34e56");
}

// Regression test: applying a remote edit whose FullOffset range partially
// overlaps a fragment that was already deleted (observed but not visible)
// used to leave the fragment unsplit, causing the rope builder to read past
// the end of the rope.
#[test]
fn test_edit_partially_intersecting_a_deleted_fragment() {
    let mut buffer = Buffer::new(ReplicaId::new(1), BufferId::new(1).unwrap(), "abcdefgh");

    // Delete "cde", creating a single deleted fragment at FullOffset 2..5.
    // After this the fragment layout is:
    //   "ab"(vis, FullOffset 0..2)  "cde"(del, 2..5)  "fgh"(vis, 5..8)
    buffer.edit([(2..5, "")]);
    assert_eq!(buffer.text(), "abfgh");

    // Construct a synthetic remote edit whose version includes the deletion (so
    // the "cde" fragment is observed + deleted → !was_visible) but whose
    // FullOffset range only partially overlaps it. This state arises in
    // production when concurrent edits cause different fragment splits on
    // different replicas.
    let synthetic_timestamp = clock::Lamport {
        replica_id: ReplicaId::new(2),
        value: 10,
    };
    let synthetic_edit = Operation::Edit(EditOperation {
        timestamp: synthetic_timestamp,
        version: buffer.version(),
        // Range 1..4 partially overlaps the deleted "cde" (FullOffset 2..5):
        // it covers "b" (1..2) and only "cd" (2..4), leaving "e" (4..5) out.
        ranges: vec![FullOffset(1)..FullOffset(4)],
        new_text: vec!["".into()],
    });

    // Without the fix this panics with "cannot summarize past end of rope"
    // because the full 3-byte "cde" fragment is consumed from the deleted
    // rope instead of only the 2-byte intersection.
    buffer.apply_ops([synthetic_edit]);
    assert_eq!(buffer.text(), "afgh");

    buffer.undo_operations([(synthetic_timestamp, u32::MAX)].into_iter().collect());
    assert_eq!(buffer.text(), "abfgh");
}

#[gpui::test(iterations = 100)]
fn test_random_concurrent_edits(mut rng: StdRng) {
    let peers = env::var("PEERS")
        .map(|i| i.parse().expect("invalid `PEERS` variable"))
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
    let mut network = Network::new(rng.clone());

    for i in 0..peers {
        let mut buffer = Buffer::new(
            ReplicaId::new(i as u16),
            BufferId::new(1).unwrap(),
            base_text.clone(),
        );
        buffer.history.group_interval = Duration::from_millis(rng.random_range(0..=200));
        buffers.push(buffer);
        replica_ids.push(ReplicaId::new(i as u16));
        network.add_peer(ReplicaId::new(i as u16));
    }

    log::info!("initial text: {:?}", base_text);

    let mut mutation_count = operations;
    loop {
        let replica_index = rng.random_range(0..peers);
        let replica_id = replica_ids[replica_index];
        let buffer = &mut buffers[replica_index];
        match rng.random_range(0..=100) {
            0..=50 if mutation_count != 0 => {
                let op = buffer.randomly_edit(&mut rng, 5).1;
                network.broadcast(buffer.replica_id, vec![op]);
                log::info!("buffer {:?} text: {:?}", buffer.replica_id, buffer.text());
                mutation_count -= 1;
            }
            51..=70 if mutation_count != 0 => {
                let ops = buffer.randomly_undo_redo(&mut rng);
                network.broadcast(buffer.replica_id, ops);
                mutation_count -= 1;
            }
            71..=100 if network.has_unreceived(replica_id) => {
                let ops = network.receive(replica_id);
                if !ops.is_empty() {
                    log::info!(
                        "peer {:?} applying {} ops from the network.",
                        replica_id,
                        ops.len()
                    );
                    buffer.apply_ops(ops);
                }
            }
            _ => {}
        }
        buffer.check_invariants();

        if mutation_count == 0 && network.is_idle() {
            break;
        }
    }

    let first_buffer = &buffers[0];
    for buffer in &buffers[1..] {
        assert_eq!(
            buffer.text(),
            first_buffer.text(),
            "Replica {:?} text != Replica 0 text",
            buffer.replica_id
        );
        buffer.check_invariants();
    }
}
