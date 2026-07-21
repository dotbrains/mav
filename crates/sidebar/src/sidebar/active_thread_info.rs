use super::*;

pub(crate) fn all_thread_infos_for_workspace(
    workspace: &Entity<Workspace>,
    cx: &App,
) -> impl Iterator<Item = ActiveThreadInfo> {
    workspace
        .read(cx)
        .items_of_type::<AgentThreadItem>(cx)
        .filter_map(|item| item.read(cx).active_thread_info(cx))
        .map(|info| ActiveThreadInfo {
            session_id: info.session_id,
            title: info.title,
            status: info.status,
            icon: info.icon,
            icon_from_external_svg: info.icon_from_external_svg,
            is_background: false,
            is_title_generating: info.is_title_generating,
            diff_stats: info.diff_stats,
        })
}
