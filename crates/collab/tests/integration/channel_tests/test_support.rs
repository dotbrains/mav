use super::*;

pub(super) fn assert_participants_eq(participants: &[Arc<User>], expected_partitipants: &[u64]) {
    assert_eq!(
        participants.iter().map(|p| p.legacy_id).collect::<Vec<_>>(),
        expected_partitipants
    );
}

#[track_caller]
pub(super) fn assert_members_eq(
    members: &[ChannelMembership],
    expected_members: &[(u64, proto::ChannelRole, proto::channel_member::Kind)],
) {
    assert_eq!(
        members
            .iter()
            .map(|member| (member.user.legacy_id, member.role, member.kind))
            .collect::<Vec<_>>(),
        expected_members
    );
}

pub(super) struct ExpectedChannel {
    pub(super) depth: usize,
    pub(super) id: ChannelId,
    pub(super) name: SharedString,
}

#[track_caller]
pub(super) fn assert_channel_invitations(
    channel_store: &Entity<ChannelStore>,
    cx: &TestAppContext,
    expected_channels: &[ExpectedChannel],
) {
    let actual = cx.read(|cx| {
        channel_store.read_with(cx, |store, _| {
            store
                .channel_invitations()
                .iter()
                .map(|channel| ExpectedChannel {
                    depth: 0,
                    name: channel.name.clone(),
                    id: channel.id,
                })
                .collect::<Vec<_>>()
        })
    });
    assert_eq!(actual, expected_channels);
}

#[track_caller]
pub(super) fn assert_channels(
    channel_store: &Entity<ChannelStore>,
    cx: &TestAppContext,
    expected_channels: &[ExpectedChannel],
) {
    let actual = cx.read(|cx| {
        channel_store.read_with(cx, |store, _| {
            store
                .ordered_channels()
                .map(|(depth, channel)| ExpectedChannel {
                    depth,
                    name: channel.name.clone(),
                    id: channel.id,
                })
                .collect::<Vec<_>>()
        })
    });
    pretty_assertions::assert_eq!(actual, expected_channels);
}

#[track_caller]
pub(super) fn assert_channels_list_shape(
    channel_store: &Entity<ChannelStore>,
    cx: &TestAppContext,
    expected_channels: &[(ChannelId, usize)],
) {
    let actual = cx.read(|cx| {
        channel_store.read_with(cx, |store, _| {
            store
                .ordered_channels()
                .map(|(depth, channel)| (channel.id, depth))
                .collect::<Vec<_>>()
        })
    });
    pretty_assertions::assert_eq!(actual, expected_channels);
}
