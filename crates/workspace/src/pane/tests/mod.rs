use std::{cell::Cell, iter::zip, num::NonZero, rc::Rc};

use super::*;
use crate::{
    Member,
    item::test::{TestItem, TestProjectItem},
};
use gpui::{
    AppContext, Axis, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    TestAppContext, VisualTestContext, size,
};
use project::FakeFs;
use settings::SettingsStore;
use theme::LoadThemes;
use util::TryFutureExt;

// drop_call_count is a Cell here because `handle_drop` takes &self, not &mut self.
struct CustomDropHandlingItem {
    focus_handle: gpui::FocusHandle,
    drop_call_count: Cell<usize>,
}

impl CustomDropHandlingItem {
    fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            drop_call_count: Cell::new(0),
        }
    }

    fn drop_call_count(&self) -> usize {
        self.drop_call_count.get()
    }
}

impl EventEmitter<()> for CustomDropHandlingItem {}

impl Focusable for CustomDropHandlingItem {
    fn focus_handle(&self, _cx: &App) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for CustomDropHandlingItem {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl gpui::IntoElement {
        gpui::Empty
    }
}

impl Item for CustomDropHandlingItem {
    type Event = ();

    fn tab_content_text(&self, _detail: usize, _cx: &App) -> gpui::SharedString {
        "custom_drop_handling_item".into()
    }

    fn handle_drop(
        &self,
        _active_pane: &Pane,
        dropped: &dyn std::any::Any,
        _window: &mut Window,
        _cx: &mut App,
    ) -> bool {
        let is_dragged_tab = dropped.downcast_ref::<DraggedTab>().is_some();
        if is_dragged_tab {
            self.drop_call_count.set(self.drop_call_count.get() + 1);
        }
        is_dragged_tab
    }
}

fn init_test(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let settings_store = SettingsStore::test(cx);
        cx.set_global(settings_store);
        theme_settings::init(LoadThemes::JustBase, cx);
    });
}

fn set_max_tabs(cx: &mut TestAppContext, value: Option<usize>) {
    cx.update_global(|store: &mut SettingsStore, cx| {
        store.update_user_settings(cx, |settings| {
            settings.workspace.max_tabs = value.map(|v| NonZero::new(v).unwrap())
        });
    });
}

fn set_pinned_tabs_separate_row(cx: &mut TestAppContext, enabled: bool) {
    cx.update_global(|store: &mut SettingsStore, cx| {
        store.update_user_settings(cx, |settings| {
            settings
                .tab_bar
                .get_or_insert_default()
                .show_pinned_tabs_in_separate_row = Some(enabled);
        });
    });
}

fn add_labeled_item(
    pane: &Entity<Pane>,
    label: &str,
    is_dirty: bool,
    cx: &mut VisualTestContext,
) -> Box<Entity<TestItem>> {
    pane.update_in(cx, |pane, window, cx| {
        let labeled_item =
            Box::new(cx.new(|cx| TestItem::new(cx).with_label(label).with_dirty(is_dirty)));
        pane.add_item(labeled_item.clone(), false, false, None, window, cx);
        labeled_item
    })
}

fn set_labeled_items<const COUNT: usize>(
    pane: &Entity<Pane>,
    labels: [&str; COUNT],
    cx: &mut VisualTestContext,
) -> [Box<Entity<TestItem>>; COUNT] {
    pane.update_in(cx, |pane, window, cx| {
        pane.items.clear();
        let mut active_item_index = 0;

        let mut index = 0;
        let items = labels.map(|mut label| {
            if label.ends_with('*') {
                label = label.trim_end_matches('*');
                active_item_index = index;
            }

            let labeled_item = Box::new(cx.new(|cx| TestItem::new(cx).with_label(label)));
            pane.add_item(labeled_item.clone(), false, false, None, window, cx);
            index += 1;
            labeled_item
        });

        pane.activate_item(active_item_index, false, false, window, cx);

        items
    })
}

// Assert the item label, with the active item label suffixed with a '*'
#[track_caller]
fn assert_item_labels<const COUNT: usize>(
    pane: &Entity<Pane>,
    expected_states: [&str; COUNT],
    cx: &mut VisualTestContext,
) {
    let actual_states = pane.update(cx, |pane, cx| {
        pane.items
            .iter()
            .enumerate()
            .map(|(ix, item)| {
                let mut state = item
                    .to_any_view()
                    .downcast::<TestItem>()
                    .unwrap()
                    .read(cx)
                    .label
                    .clone();
                if ix == pane.active_item_index {
                    state.push('*');
                }
                if item.is_dirty(cx) {
                    state.push('^');
                }
                if pane.is_tab_pinned(ix) {
                    state.push('!');
                }
                state
            })
            .collect::<Vec<_>>()
    });
    assert_eq!(
        actual_states, expected_states,
        "pane items do not match expectation"
    );
}

// Assert the item label, with the active item label expected active index
#[track_caller]
fn assert_item_labels_active_index(
    pane: &Entity<Pane>,
    expected_states: &[&str],
    expected_active_idx: usize,
    cx: &mut VisualTestContext,
) {
    let actual_states = pane.update(cx, |pane, cx| {
        pane.items
            .iter()
            .enumerate()
            .map(|(ix, item)| {
                let mut state = item
                    .to_any_view()
                    .downcast::<TestItem>()
                    .unwrap()
                    .read(cx)
                    .label
                    .clone();
                if ix == pane.active_item_index {
                    assert_eq!(ix, expected_active_idx);
                }
                if item.is_dirty(cx) {
                    state.push('^');
                }
                if pane.is_tab_pinned(ix) {
                    state.push('!');
                }
                state
            })
            .collect::<Vec<_>>()
    });
    assert_eq!(
        actual_states, expected_states,
        "pane items do not match expectation"
    );
}

#[track_caller]
fn assert_pane_ids_on_axis<const COUNT: usize>(
    workspace: &Entity<Workspace>,
    expected_ids: [&EntityId; COUNT],
    expected_axis: Axis,
    cx: &mut VisualTestContext,
) {
    workspace.read_with(cx, |workspace, _| match &workspace.center.root {
        Member::Axis(axis) => {
            assert_eq!(axis.axis, expected_axis);
            assert_eq!(axis.members.len(), expected_ids.len());
            assert!(
                zip(expected_ids, &axis.members).all(|(e, a)| {
                    if let Member::Pane(p) = a {
                        p.entity_id() == *e
                    } else {
                        false
                    }
                }),
                "pane ids do not match expectation: {expected_ids:?} != {actual_ids:?}",
                actual_ids = axis.members
            );
        }
        Member::Pane(_) => panic!("expected axis"),
    });
}

async fn test_single_pane_split<const COUNT: usize>(
    pane_labels: [&str; COUNT],
    direction: SplitDirection,
    operation: SplitMode,
    cx: &mut TestAppContext,
) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) = cx.add_window_view(|window, cx| Workspace::test_new(project, window, cx));

    let mut pane_before = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());
    for label in pane_labels {
        add_labeled_item(&pane_before, label, false, cx);
    }
    pane_before.update_in(cx, |pane, window, cx| {
        pane.split(direction, operation, window, cx)
    });
    cx.executor().run_until_parked();
    let pane_after = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    let num_labels = pane_labels.len();
    let last_as_active = format!("{}*", String::from(pane_labels[num_labels - 1]));

    // check labels for all split operations
    match operation {
        SplitMode::EmptyPane => {
            assert_item_labels_active_index(&pane_before, &pane_labels, num_labels - 1, cx);
            assert_item_labels(&pane_after, [], cx);
        }
        SplitMode::ClonePane => {
            assert_item_labels_active_index(&pane_before, &pane_labels, num_labels - 1, cx);
            assert_item_labels(&pane_after, [&last_as_active], cx);
        }
        SplitMode::MovePane => {
            let head = &pane_labels[..(num_labels - 1)];
            if num_labels == 1 {
                // We special-case this behavior and actually execute an empty pane command
                // followed by a refocus of the old pane for this case.
                pane_before = workspace.read_with(cx, |workspace, _cx| {
                    workspace
                        .panes()
                        .into_iter()
                        .find(|pane| *pane != &pane_after)
                        .unwrap()
                        .clone()
                });
            };

            assert_item_labels_active_index(&pane_before, &head, head.len().saturating_sub(1), cx);
            assert_item_labels(&pane_after, [&last_as_active], cx);
            pane_after.update_in(cx, |pane, window, cx| {
                window.focused(cx).is_some_and(|focus_handle| {
                    focus_handle == pane.active_item().unwrap().item_focus_handle(cx)
                })
            });
        }
    }

    // expected axis depends on split direction
    let expected_axis = match direction {
        SplitDirection::Right | SplitDirection::Left => Axis::Horizontal,
        SplitDirection::Up | SplitDirection::Down => Axis::Vertical,
    };

    // expected ids depends on split direction
    let expected_ids = match direction {
        SplitDirection::Right | SplitDirection::Down => {
            [&pane_before.entity_id(), &pane_after.entity_id()]
        }
        SplitDirection::Left | SplitDirection::Up => {
            [&pane_after.entity_id(), &pane_before.entity_id()]
        }
    };

    // check pane axes for all operations
    match operation {
        SplitMode::EmptyPane | SplitMode::ClonePane => {
            assert_pane_ids_on_axis(&workspace, expected_ids, expected_axis, cx);
        }
        SplitMode::MovePane => {
            assert_pane_ids_on_axis(&workspace, expected_ids, expected_axis, cx);
        }
    }
}

mod close_all;
mod close_commands;
mod drag_drop_pinning;
mod drag_drop_rows;
mod drag_drop_target;
mod item_ordering;
mod max_tabs;
mod navigation_property;
mod pinned_rows;
mod save_reload;
mod scroll_and_pinned_close;
mod split_and_activation;
mod tab_interactions;
