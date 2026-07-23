use super::*;

pub(super) fn assert_channel_tree(actual: Vec<Channel>, expected: &[(ChannelId, &[ChannelId])]) {
    let actual = actual
        .iter()
        .map(|channel| (channel.id, channel.parent_path.as_slice()))
        .collect::<HashSet<_>>();
    let expected = expected
        .iter()
        .map(|(id, parents)| (*id, *parents))
        .collect::<HashSet<_>>();
    pretty_assertions::assert_eq!(actual, expected, "wrong channel ids and parent paths");
}

#[track_caller]
pub(super) fn assert_channel_tree_order(
    actual: Vec<Channel>,
    expected: &[(ChannelId, &[ChannelId], i32)],
) {
    let actual = actual
        .iter()
        .map(|channel| {
            (
                channel.id,
                channel.parent_path.as_slice(),
                channel.channel_order,
            )
        })
        .collect::<HashSet<_>>();
    let expected = expected
        .iter()
        .map(|(id, parents, order)| (*id, *parents, *order))
        .collect::<HashSet<_>>();
    pretty_assertions::assert_eq!(actual, expected, "wrong channel ids and parent paths");
}
