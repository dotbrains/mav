use super::*;
use proptest::prelude::*;
use serde_json::json;
use std::{collections::HashSet, sync::Arc};
use util::{
    path,
    rel_path::{RelPath, rel_path},
};

struct TestFileItem {
    project_path: ProjectPath,
}

impl project::ProjectItem for TestFileItem {
    fn try_open(
        _project: &Entity<Project>,
        path: &ProjectPath,
        cx: &mut App,
    ) -> Option<Task<anyhow::Result<Entity<Self>>>> {
        let project_path = path.clone();
        Some(cx.spawn(async move |cx| Ok(cx.new(|_| Self { project_path }))))
    }

    fn entry_id(&self, _: &App) -> Option<ProjectEntryId> {
        None
    }

    fn project_path(&self, _: &App) -> Option<ProjectPath> {
        Some(self.project_path.clone())
    }

    fn is_dirty(&self) -> bool {
        false
    }
}

struct TestItemView {
    focus_handle: FocusHandle,
    project_item: Entity<TestFileItem>,
    nav_history: Option<ItemNavHistory>,
}

impl EventEmitter<()> for TestItemView {}

impl Focusable for TestItemView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for TestItemView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        gpui::Empty
    }
}

impl Item for TestItemView {
    type Event = ();

    fn tab_content_text(&self, _detail: usize, _cx: &App) -> SharedString {
        "".into()
    }

    fn for_each_project_item(
        &self,
        cx: &App,
        f: &mut dyn FnMut(EntityId, &dyn project::ProjectItem),
    ) {
        f(self.project_item.entity_id(), self.project_item.read(cx))
    }

    fn buffer_kind(&self, _: &App) -> ItemBufferKind {
        ItemBufferKind::Singleton
    }

    fn set_nav_history(
        &mut self,
        history: ItemNavHistory,
        _window: &mut Window,
        _: &mut Context<Self>,
    ) {
        self.nav_history = Some(history);
    }

    fn deactivated(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(nav_history) = self.nav_history.as_mut() {
            nav_history.push::<()>(None, None, cx);
        }
    }
}

impl crate::ProjectItem for TestItemView {
    type Item = TestFileItem;

    fn for_project_item(
        _project: Entity<Project>,
        _pane: Option<&Pane>,
        item: Entity<Self::Item>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self
    where
        Self: Sized,
    {
        Self {
            focus_handle: cx.focus_handle(),
            project_item: item,
            nav_history: None,
        }
    }
}

fn arbitrary_path() -> impl Strategy<Value = Arc<RelPath>> {
    prop_oneof![
        Just(rel_path("1.txt").into()),
        Just(rel_path("2.js").into()),
        Just(rel_path("3.rs").into()),
    ]
}

#[derive(Debug, Clone, proptest_derive::Arbitrary)]
enum Operation {
    Open {
        #[proptest(strategy = "arbitrary_path()")]
        path: Arc<RelPath>,
        allow_preview: bool,
    },
    GoBack,
    GoForward,
}

struct Oracle {
    /// The active item's path, if known.
    current: Option<Arc<RelPath>>,
    /// The path that the back button would navigate to, if known.
    previous: Option<Arc<RelPath>>,
    /// The path that the forward button would navigate to, if known.
    next: Option<Arc<RelPath>>,
}

impl Oracle {
    fn new() -> Self {
        Self {
            current: None,
            previous: None,
            next: None,
        }
    }

    fn apply(&mut self, operation: Operation) {
        match operation {
            Operation::Open { path, .. } => {
                if self.current.as_ref() != Some(&path) {
                    self.previous = self.current.replace(path);
                    self.next = None;
                }
            }
            Operation::GoBack => {
                if let Some(previous) = self.previous.take() {
                    self.next = self.current.replace(previous);
                } else {
                    // `previous` isn't set, so backward navigation may not have been
                    // possible, hence we don't know which item a following forward
                    // navigation will lead to
                    self.next = None;
                    self.current = None;
                }
            }
            Operation::GoForward => {
                if let Some(next) = self.next.take() {
                    self.previous = self.current.replace(next);
                } else {
                    self.previous = None;
                    self.current = None;
                }
            }
        }
    }
}

struct PaneHarness {
    cx: VisualTestContext,
    workspace: Entity<Workspace>,
    pane: Entity<Pane>,
    worktree_id: WorktreeId,
}

impl PaneHarness {
    async fn new(cx: &mut TestAppContext) -> Self {
        init_test(cx);
        cx.update(|cx| crate::register_project_item::<TestItemView>(cx));

        let fs = FakeFs::new(cx.executor());
        fs.insert_tree(
            path!("/root"),
            json!({
                "1.txt": "one",
                "2.js": "two",
                "3.rs": "three",
            }),
        )
        .await;
        let project = Project::test(fs, [path!("/root").as_ref()], cx).await;
        let worktree_id = project.update(cx, |project, cx| {
            project.worktrees(cx).next().unwrap().read(cx).id()
        });
        let window = cx.add_window(|window, cx| Workspace::test_new(project, window, cx));
        let workspace = window.root(cx).unwrap();
        let cx = VisualTestContext::from_window(*window, cx);
        let pane = workspace.read_with(&cx, |workspace, _| workspace.active_pane().clone());

        Self {
            cx,
            workspace,
            pane,
            worktree_id,
        }
    }

    async fn apply(&mut self, operation: Operation) {
        match operation {
            Operation::Open {
                path,
                allow_preview,
            } => {
                self.workspace
                    .update_in(&mut self.cx, |workspace, window, cx| {
                        workspace.open_path_preview_in_tabbed_pane(
                            ProjectPath {
                                worktree_id: self.worktree_id,
                                path,
                            },
                            None,
                            true,
                            allow_preview,
                            true,
                            window,
                            cx,
                        )
                    })
                    .await
                    .unwrap();
            }
            Operation::GoBack => {
                self.workspace
                    .update_in(&mut self.cx, |workspace, window, cx| {
                        workspace.go_back(self.pane.downgrade(), window, cx)
                    })
                    .await
                    .unwrap();
            }
            Operation::GoForward => {
                self.workspace
                    .update_in(&mut self.cx, |workspace, window, cx| {
                        workspace.go_forward(self.pane.downgrade(), window, cx)
                    })
                    .await
                    .unwrap();
            }
        }
    }

    fn check_invariants(&self, expected_path: &Option<Arc<RelPath>>) {
        self.pane.read_with(&self.cx, |pane, cx| {
            let open_paths = pane
                .items()
                .map(|item| item.project_path(cx).unwrap())
                .collect::<Vec<_>>();
            let active_path = pane
                .active_item()
                .map(|item| item.project_path(cx).unwrap());
            let preview_path = pane
                .preview_item()
                .map(|item| item.project_path(cx).unwrap());

            let unique_paths = open_paths.iter().collect::<HashSet<_>>();
            assert_eq!(
                unique_paths.len(),
                open_paths.len(),
                "pane should not contain duplicate open paths"
            );

            assert_eq!(
                active_path.is_none(),
                open_paths.is_empty(),
                "pane should have an active item iff it has open paths"
            );
            assert!(
                active_path
                    .as_ref()
                    .is_none_or(|path| open_paths.contains(path)),
                "active path should be open"
            );
            assert!(
                preview_path
                    .as_ref()
                    .is_none_or(|path| open_paths.contains(path)),
                "preview path should be open"
            );

            if let Some(expected_active_path) = expected_path {
                assert_eq!(
                    &active_path.as_ref().unwrap().path,
                    expected_active_path,
                    "active path should match the oracle"
                );
            }
        });
    }
}

#[gpui::property_test]
async fn single_pane_navigation(
    #[strategy = proptest::collection::vec(any::<Operation>(), 1..32)] operations: Vec<Operation>,
    cx: &mut TestAppContext,
) {
    let mut harness = PaneHarness::new(cx).await;
    let mut oracle = Oracle::new();

    for operation in operations {
        oracle.apply(operation.clone());
        harness.apply(operation).await;
        harness.check_invariants(&oracle.current);
    }
}
