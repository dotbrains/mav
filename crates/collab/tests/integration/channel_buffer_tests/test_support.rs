use super::*;

pub(super) fn assert_remote_selections(
    editor: &mut Editor,
    expected_selections: &[(Option<ParticipantIndex>, Range<usize>)],
    window: &mut Window,
    cx: &mut Context<Editor>,
) {
    let snapshot = editor.snapshot(window, cx);
    let hub = editor.collaboration_hub().unwrap();
    let collaborators = hub.collaborators(cx);
    let range = Anchor::Min..Anchor::Max;
    let remote_selections = snapshot
        .remote_selections_in_range(&range, hub, cx)
        .map(|s| {
            let CollaboratorId::PeerId(peer_id) = s.collaborator_id else {
                panic!("unexpected collaborator id");
            };
            let start = s.selection.start.to_offset(snapshot.buffer_snapshot());
            let end = s.selection.end.to_offset(snapshot.buffer_snapshot());
            let user_id = collaborators.get(&peer_id).unwrap().user_id;
            let participant_index = hub.user_participant_indices(cx).get(&user_id).copied();
            (participant_index, start.0..end.0)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        remote_selections, expected_selections,
        "incorrect remote selections"
    );
}

pub(super) fn assert_collaborators(
    collaborators: &HashMap<PeerId, Collaborator>,
    ids: &[Option<LegacyUserId>],
) {
    let mut user_ids = collaborators
        .values()
        .map(|collaborator| collaborator.user_id)
        .collect::<Vec<_>>();
    user_ids.sort();
    assert_eq!(
        user_ids,
        ids.iter().map(|id| id.unwrap()).collect::<Vec<_>>()
    );
}

pub(super) fn buffer_text(
    channel_buffer: &Entity<language::Buffer>,
    cx: &mut TestAppContext,
) -> String {
    channel_buffer.read_with(cx, |buffer, _| buffer.text())
}
