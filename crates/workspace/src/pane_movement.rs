use gpui::{App, Entity, EntityId, Focusable};
use ui::Window;
use util::ResultExt;

use crate::{Pane, WorkspaceId, pane};

pub(super) fn join_pane_into_active(
    active_pane: &Entity<Pane>,
    pane: &Entity<Pane>,
    window: &mut Window,
    cx: &mut App,
) {
    if pane == active_pane {
    } else if pane.read(cx).items_len() == 0 {
        pane.update(cx, |_, cx| {
            cx.emit(pane::Event::Remove {
                focus_on_pane: None,
            });
        })
    } else {
        move_all_items(pane, active_pane, window, cx);
    }
}

pub(super) fn move_all_items(
    from_pane: &Entity<Pane>,
    to_pane: &Entity<Pane>,
    window: &mut Window,
    cx: &mut App,
) {
    if !from_pane.read(cx).is_tabbed() || !to_pane.read(cx).is_tabbed() {
        return;
    }

    let destination_is_different = from_pane != to_pane;
    let mut moved_items = 0;
    for (item_ix, item_handle) in from_pane
        .read(cx)
        .items()
        .enumerate()
        .map(|(ix, item)| (ix, item.clone()))
        .collect::<Vec<_>>()
    {
        let ix = item_ix - moved_items;
        if destination_is_different {
            from_pane.update(cx, |source, cx| {
                source.remove_item_and_focus_on_pane(ix, false, to_pane.clone(), window, cx);
            });
            moved_items += 1;
        }

        to_pane.update(cx, |destination, cx| {
            destination.add_item(item_handle, true, true, None, window, cx);
            window.focus(&destination.focus_handle(cx), cx)
        });
    }
}

pub fn move_item(
    source: &Entity<Pane>,
    destination: &Entity<Pane>,
    item_id_to_move: EntityId,
    destination_index: usize,
    activate: bool,
    window: &mut Window,
    cx: &mut App,
) {
    if !source.read(cx).is_tabbed() || !destination.read(cx).is_tabbed() {
        return;
    }

    let Some((item_ix, item_handle)) = source
        .read(cx)
        .items()
        .enumerate()
        .find(|(_, item_handle)| item_handle.item_id() == item_id_to_move)
        .map(|(ix, item)| (ix, item.clone()))
    else {
        return;
    };

    if source != destination {
        source.update(cx, |source, cx| {
            source.remove_item_and_focus_on_pane(item_ix, false, destination.clone(), window, cx);
        });
    }

    destination.update(cx, |destination, cx| {
        destination.add_item_inner(
            item_handle,
            activate,
            activate,
            activate,
            Some(destination_index),
            window,
            cx,
        );
        if activate {
            window.focus(&destination.focus_handle(cx), cx)
        }
    });
}

pub fn move_active_item(
    source: &Entity<Pane>,
    destination: &Entity<Pane>,
    focus_destination: bool,
    close_if_empty: bool,
    window: &mut Window,
    cx: &mut App,
) {
    if source == destination {
        return;
    }
    if !source.read(cx).is_tabbed() || !destination.read(cx).is_tabbed() {
        return;
    }
    let Some(active_item) = source.read(cx).active_item() else {
        return;
    };
    source.update(cx, |source_pane, cx| {
        let item_id = active_item.item_id();
        source_pane.remove_item(item_id, false, close_if_empty, window, cx);
        destination.update(cx, |target_pane, cx| {
            target_pane.add_item(
                active_item,
                focus_destination,
                focus_destination,
                Some(target_pane.items_len()),
                window,
                cx,
            );
        });
    });
}

pub fn clone_active_item(
    workspace_id: Option<WorkspaceId>,
    source: &Entity<Pane>,
    destination: &Entity<Pane>,
    focus_destination: bool,
    window: &mut Window,
    cx: &mut App,
) {
    if source == destination {
        return;
    }
    if !source.read(cx).is_tabbed() || !destination.read(cx).is_tabbed() {
        return;
    }
    let Some(active_item) = source.read(cx).active_item() else {
        return;
    };
    if !active_item.can_split(cx) {
        return;
    }
    let destination = destination.downgrade();
    let task = active_item.clone_on_split(workspace_id, window, cx);
    window
        .spawn(cx, async move |cx| {
            let Some(clone) = task.await else {
                return;
            };
            destination
                .update_in(cx, |target_pane, window, cx| {
                    target_pane.add_item(
                        clone,
                        focus_destination,
                        focus_destination,
                        Some(target_pane.items_len()),
                        window,
                        cx,
                    );
                })
                .log_err();
        })
        .detach();
}
