use super::*;
use crate::git::blame::{BlameRenderer, GitBlame, GlobalBlameRenderer};
use git::{Oid, commit::ParsedCommitMessage};

pub(super) fn render_inline_blame_entry(
    blame_entry: BlameEntry,
    style: &EditorStyle,
    cx: &mut App,
) -> Option<AnyElement> {
    let renderer = cx.global::<GlobalBlameRenderer>().0.clone();
    renderer.render_inline_blame_entry(&style.text, blame_entry, cx)
}

pub(super) fn render_blame_entry_popover(
    blame_entry: BlameEntry,
    scroll_handle: ScrollHandle,
    commit_message: Option<ParsedCommitMessage>,
    markdown: Entity<Markdown>,
    workspace: WeakEntity<Workspace>,
    blame: &Entity<GitBlame>,
    buffer: BufferId,
    window: &mut Window,
    cx: &mut App,
) -> Option<AnyElement> {
    if markdown.read(cx).is_parsing() {
        return None;
    }

    let renderer = cx.global::<GlobalBlameRenderer>().0.clone();
    let blame = blame.read(cx);
    let repository = blame.repository(cx, buffer)?;
    renderer.render_blame_entry_popover(
        blame_entry,
        scroll_handle,
        commit_message,
        markdown,
        repository,
        workspace,
        window,
        cx,
    )
}

pub(super) fn render_blame_entry(
    ix: usize,
    blame: &Entity<GitBlame>,
    blame_entry: BlameEntry,
    style: &EditorStyle,
    last_used_color: &mut Option<(Hsla, Oid)>,
    editor: Entity<Editor>,
    workspace: Entity<Workspace>,
    buffer: BufferId,
    renderer: &dyn BlameRenderer,
    window: &mut Window,
    cx: &mut App,
) -> Option<AnyElement> {
    let index: u32 = blame_entry.sha.into();
    let mut sha_color = cx.theme().players().color_for_participant(index).cursor;

    if let Some((color, sha)) = *last_used_color
        && sha != blame_entry.sha
        && color == sha_color
    {
        sha_color = cx.theme().players().color_for_participant(index + 1).cursor;
    }
    last_used_color.replace((sha_color, blame_entry.sha));

    let blame = blame.read(cx);
    let details = blame.details_for_entry(buffer, &blame_entry);
    let repository = blame.repository(cx, buffer)?;
    renderer.render_blame_entry(
        &style.text,
        blame_entry,
        details,
        repository,
        workspace.downgrade(),
        editor,
        ix,
        sha_color,
        window,
        cx,
    )
}
