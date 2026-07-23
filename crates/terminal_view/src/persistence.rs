use anyhow::Result;
use async_recursion::async_recursion;
use collections::HashSet;
use futures::future::join_all;
use gpui::{AppContext as _, AsyncWindowContext, Entity, Task, WeakEntity};
use project::Project;
use ui::{App, Context, Window};
use util::ResultExt as _;

use ::db::kvp::KeyValueStore;
use workspace::{
    ItemHandle, Member, Pane, PaneAxis, PaneGroup, SerializableItem as _, Workspace, WorkspaceId,
};

use crate::{
    TerminalView, default_working_directory,
    terminal_panel::{TerminalPanel, new_terminal_pane},
};

const TERMINAL_PANEL_KEY: &str = "TerminalPanel";

pub(crate) fn serialize_pane_group(
    pane_group: &PaneGroup,
    active_pane: &Entity<Pane>,
    cx: &mut App,
) -> SerializedPaneGroup {
    build_serialized_pane_group(&pane_group.root, active_pane, cx)
}

fn build_serialized_pane_group(
    pane_group: &Member,
    active_pane: &Entity<Pane>,
    cx: &mut App,
) -> SerializedPaneGroup {
    match pane_group {
        Member::Axis(PaneAxis {
            axis,
            members,
            flexes,
            bounding_boxes: _,
        }) => SerializedPaneGroup::Group {
            axis: SerializedAxis(*axis),
            children: members
                .iter()
                .map(|member| build_serialized_pane_group(member, active_pane, cx))
                .collect::<Vec<_>>(),
            flexes: Some(flexes.lock().clone()),
        },
        Member::Pane(pane_handle) => {
            SerializedPaneGroup::Pane(serialize_pane(pane_handle, pane_handle == active_pane, cx))
        }
    }
}

fn serialize_pane(pane: &Entity<Pane>, active: bool, cx: &mut App) -> SerializedPane {
    let mut items_to_serialize = HashSet::default();
    let pane = pane.read(cx);
    let children = pane
        .items()
        .filter_map(|item| {
            let terminal_view = item.act_as::<TerminalView>(cx)?;
            if terminal_view.read(cx).terminal().read(cx).task().is_some() {
                None
            } else {
                let id = item.item_id().as_u64();
                items_to_serialize.insert(id);
                Some(id)
            }
        })
        .collect::<Vec<_>>();
    let active_item = pane
        .active_item()
        .map(|item| item.item_id().as_u64())
        .filter(|active_id| items_to_serialize.contains(active_id));

    let pinned_count = pane.pinned_count();
    SerializedPane {
        active,
        children,
        active_item,
        pinned_count,
    }
}

pub(crate) fn deserialize_terminal_panel(
    workspace: WeakEntity<Workspace>,
    project: Entity<Project>,
    database_id: WorkspaceId,
    serialized_panel: SerializedTerminalPanel,
    window: &mut Window,
    cx: &mut App,
) -> Task<anyhow::Result<Entity<TerminalPanel>>> {
    window.spawn(cx, async move |cx| {
        let terminal_panel = workspace.update_in(cx, |workspace, window, cx| {
            cx.new(|cx| TerminalPanel::new(workspace, window, cx))
        })?;
        match &serialized_panel.items {
            SerializedItems::NoSplits(item_ids) => {
                let items = deserialize_terminal_views(
                    database_id,
                    project,
                    workspace,
                    item_ids.as_slice(),
                    cx,
                )
                .await;
                let active_item = serialized_panel.active_item_id;
                terminal_panel.update_in(cx, |terminal_panel, window, cx| {
                    terminal_panel.active_pane.update(cx, |pane, cx| {
                        populate_pane_items(pane, items, active_item, window, cx);
                    });
                })?;
            }
            SerializedItems::WithSplits(serialized_pane_group) => {
                let center_pane = deserialize_pane_group(
                    workspace,
                    project,
                    terminal_panel.clone(),
                    database_id,
                    serialized_pane_group,
                    cx,
                )
                .await;
                if let Some((center_group, active_pane)) = center_pane {
                    terminal_panel.update(cx, |terminal_panel, _| {
                        terminal_panel.center = PaneGroup::with_root(center_group);
                        terminal_panel.active_pane =
                            active_pane.unwrap_or_else(|| terminal_panel.center.first_pane());
                    });
                }
            }
        }

        Ok(terminal_panel)
    })
}

fn populate_pane_items(
    pane: &mut Pane,
    items: Vec<Entity<TerminalView>>,
    active_item: Option<u64>,
    window: &mut Window,
    cx: &mut Context<Pane>,
) {
    let mut active_item_index = None;
    for (item_index, item) in (pane.items_len()..).zip(items) {
        if Some(item.item_id().as_u64()) == active_item {
            active_item_index = Some(item_index);
        }
        pane.add_item(Box::new(item), false, false, None, window, cx);
    }
    if let Some(index) = active_item_index {
        pane.activate_item(index, false, false, window, cx);
    }
}

pub(crate) fn migrate_legacy_terminal_panel(
    workspace: WeakEntity<Workspace>,
    database_id: WorkspaceId,
    serialization_key: String,
    project: Entity<Project>,
    window: &mut Window,
    cx: &mut App,
) -> Task<Result<()>> {
    let kvp = KeyValueStore::global(cx);

    window.spawn(cx, async move |cx| {
        let Some(serialized_panel) = kvp
            .read_kvp(&serialization_key)
            .log_err()
            .flatten()
            .map(|panel| serde_json::from_str::<SerializedTerminalPanel>(&panel))
            .transpose()
            .log_err()
            .flatten()
        else {
            return Ok(());
        };

        let (item_ids, active_item_id) = serialized_panel_item_ids(&serialized_panel);
        if item_ids.is_empty() {
            kvp.delete_kvp(serialization_key).await?;
            return Ok(());
        }

        let items =
            deserialize_terminal_views(database_id, project, workspace.clone(), &item_ids, cx)
                .await;

        if items.is_empty() {
            kvp.delete_kvp(serialization_key).await?;
            return Ok(());
        }

        workspace.update_in(cx, |workspace, window, cx| {
            let pane = workspace.active_pane().clone();
            let should_focus_restored_terminal = workspace.active_item(cx).is_none();
            let active_item_id = active_item_id.or_else(|| {
                items
                    .first()
                    .map(|terminal_view| terminal_view.item_id().as_u64())
            });

            for item in items {
                let focus_item = should_focus_restored_terminal
                    && Some(item.item_id().as_u64()) == active_item_id;
                workspace.add_item(
                    pane.clone(),
                    Box::new(item),
                    None,
                    false,
                    focus_item,
                    window,
                    cx,
                );
            }
        })?;

        kvp.delete_kvp(serialization_key).await?;
        Ok(())
    })
}

pub(crate) fn terminal_panel_serialization_key(workspace: &Workspace) -> Option<String> {
    workspace
        .database_id()
        .map(|id| i64::from(id).to_string())
        .or(workspace.session_id())
        .map(|id| format!("{:?}-{:?}", TERMINAL_PANEL_KEY, id))
}

fn serialized_panel_item_ids(
    serialized_panel: &SerializedTerminalPanel,
) -> (Vec<u64>, Option<u64>) {
    let mut item_ids = Vec::new();
    let mut seen_item_ids = HashSet::default();
    let mut active_item_id = serialized_panel.active_item_id;

    match &serialized_panel.items {
        SerializedItems::NoSplits(ids) => {
            for item_id in ids {
                push_unique_item_id(*item_id, &mut seen_item_ids, &mut item_ids);
            }
        }
        SerializedItems::WithSplits(group) => {
            collect_serialized_group_item_ids(
                group,
                &mut seen_item_ids,
                &mut item_ids,
                &mut active_item_id,
            );
        }
    }

    (item_ids, active_item_id)
}

fn collect_serialized_group_item_ids(
    group: &SerializedPaneGroup,
    seen_item_ids: &mut HashSet<u64>,
    item_ids: &mut Vec<u64>,
    active_item_id: &mut Option<u64>,
) {
    match group {
        SerializedPaneGroup::Pane(pane) => {
            for item_id in &pane.children {
                push_unique_item_id(*item_id, seen_item_ids, item_ids);
            }
            if pane.active {
                *active_item_id = pane
                    .active_item
                    .or_else(|| pane.children.first().copied())
                    .or(*active_item_id);
            }
        }
        SerializedPaneGroup::Group { children, .. } => {
            for child in children {
                collect_serialized_group_item_ids(child, seen_item_ids, item_ids, active_item_id);
            }
        }
    }
}

fn push_unique_item_id(item_id: u64, seen_item_ids: &mut HashSet<u64>, item_ids: &mut Vec<u64>) {
    if seen_item_ids.insert(item_id) {
        item_ids.push(item_id);
    }
}

#[async_recursion(?Send)]
async fn deserialize_pane_group(
    workspace: WeakEntity<Workspace>,
    project: Entity<Project>,
    panel: Entity<TerminalPanel>,
    workspace_id: WorkspaceId,
    serialized: &SerializedPaneGroup,
    cx: &mut AsyncWindowContext,
) -> Option<(Member, Option<Entity<Pane>>)> {
    match serialized {
        SerializedPaneGroup::Group {
            axis,
            flexes,
            children,
        } => {
            let mut current_active_pane = None;
            let mut members = Vec::new();
            for child in children {
                if let Some((new_member, active_pane)) = deserialize_pane_group(
                    workspace.clone(),
                    project.clone(),
                    panel.clone(),
                    workspace_id,
                    child,
                    cx,
                )
                .await
                {
                    members.push(new_member);
                    current_active_pane = current_active_pane.or(active_pane);
                }
            }

            if members.is_empty() {
                return None;
            }

            if members.len() == 1 {
                return Some((members.remove(0), current_active_pane));
            }

            Some((
                Member::Axis(PaneAxis::load(axis.0, members, flexes.clone())),
                current_active_pane,
            ))
        }
        SerializedPaneGroup::Pane(serialized_pane) => {
            let active = serialized_pane.active;

            let pane = panel
                .update_in(cx, |terminal_panel, window, cx| {
                    new_terminal_pane(
                        workspace.clone(),
                        project.clone(),
                        terminal_panel.active_pane.read(cx).is_zoomed(),
                        window,
                        cx,
                    )
                })
                .log_err()?;
            let active_item = serialized_pane.active_item;
            let pinned_count = serialized_pane.pinned_count;
            let new_items = deserialize_terminal_views(
                workspace_id,
                project.clone(),
                workspace.clone(),
                serialized_pane.children.as_slice(),
                cx,
            );
            cx.spawn({
                let pane = pane.downgrade();
                async move |cx| {
                    let new_items = new_items.await;

                    let items = pane.update_in(cx, |pane, window, cx| {
                        populate_pane_items(pane, new_items, active_item, window, cx);
                        pane.set_pinned_count(pinned_count.min(pane.items_len()));
                        pane.items_len()
                    });
                    // Avoid blank panes in splits
                    if items.is_ok_and(|items| items == 0) {
                        let working_directory = workspace
                            .update(cx, |workspace, cx| default_working_directory(workspace, cx))
                            .ok()
                            .flatten();
                        let terminal = project
                            .update(cx, |project, cx| {
                                project.create_terminal_shell(working_directory, cx)
                            })
                            .await
                            .log_err();
                        let Some(terminal) = terminal else {
                            return;
                        };
                        pane.update_in(cx, |pane, window, cx| {
                            let terminal_view = Box::new(cx.new(|cx| {
                                TerminalView::new(
                                    terminal,
                                    workspace.clone(),
                                    Some(workspace_id),
                                    project.downgrade(),
                                    window,
                                    cx,
                                )
                            }));
                            pane.add_item(terminal_view, true, false, None, window, cx);
                        })
                        .ok();
                    }
                }
            })
            .await;
            Some((Member::Pane(pane.clone()), active.then_some(pane)))
        }
    }
}

fn deserialize_terminal_views(
    workspace_id: WorkspaceId,
    project: Entity<Project>,
    workspace: WeakEntity<Workspace>,
    item_ids: &[u64],
    cx: &mut AsyncWindowContext,
) -> impl Future<Output = Vec<Entity<TerminalView>>> + use<> {
    let deserialized_items = join_all(item_ids.iter().filter_map(|item_id| {
        cx.update(|window, cx| {
            TerminalView::deserialize(
                project.clone(),
                workspace.clone(),
                workspace_id,
                *item_id,
                window,
                cx,
            )
        })
        .ok()
    }));
    async move {
        deserialized_items
            .await
            .into_iter()
            .filter_map(|item| item.log_err())
            .collect()
    }
}

mod db;
mod model;

pub use db::TerminalDb;
pub(crate) use model::{
    SerializedAxis, SerializedItems, SerializedPane, SerializedPaneGroup, SerializedTerminalPanel,
};
