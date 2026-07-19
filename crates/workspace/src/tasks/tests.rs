use super::*;
use crate::{
    TerminalProvider,
    item::test::{TestItem, TestProjectItem},
    register_serializable_item,
};
use gpui::{App, TestAppContext};
use parking_lot::Mutex;
use project::{FakeFs, Project, TaskSourceKind};
use serde_json::json;
use std::sync::Arc;
use task::TaskTemplate;

struct Fixture {
    workspace: Entity<Workspace>,
    item: Entity<TestItem>,
    task: ResolvedTask,
    dirty_before_spawn: Arc<Mutex<Option<bool>>>,
}

#[gpui::test]
async fn test_schedule_resolved_task_save_all(cx: &mut TestAppContext) {
    let (fixture, cx) = create_fixture(cx, SaveStrategy::All).await;
    fixture.workspace.update_in(cx, |workspace, window, cx| {
        workspace.schedule_resolved_task(
            TaskSourceKind::UserInput,
            fixture.task,
            false,
            window,
            cx,
        );
    });
    cx.executor().run_until_parked();

    assert_eq!(*fixture.dirty_before_spawn.lock(), Some(false));
    assert!(cx.read(|cx| !fixture.item.read(cx).is_dirty));
}

#[gpui::test]
async fn test_schedule_resolved_task_save_current(cx: &mut TestAppContext) {
    let (fixture, cx) = create_fixture(cx, SaveStrategy::Current).await;
    let inactive = add_test_item(&fixture.workspace, "file2.txt", false, cx);
    fixture.workspace.update_in(cx, |workspace, window, cx| {
        workspace.schedule_resolved_task(
            TaskSourceKind::UserInput,
            fixture.task,
            false,
            window,
            cx,
        );
    });
    cx.executor().run_until_parked();

    assert_eq!(*fixture.dirty_before_spawn.lock(), Some(false));
    assert!(cx.read(|cx| !fixture.item.read(cx).is_dirty));
    assert!(cx.read(|cx| inactive.read(cx).is_dirty));
}

#[gpui::test]
async fn test_schedule_resolved_task_save_none(cx: &mut TestAppContext) {
    let (fixture, cx) = create_fixture(cx, SaveStrategy::None).await;
    fixture.workspace.update_in(cx, |workspace, window, cx| {
        workspace.schedule_resolved_task(
            TaskSourceKind::UserInput,
            fixture.task,
            false,
            window,
            cx,
        );
    });
    cx.executor().run_until_parked();

    assert_eq!(*fixture.dirty_before_spawn.lock(), Some(true));
    assert!(cx.read(|cx| fixture.item.read(cx).is_dirty));
}

async fn create_fixture(
    cx: &mut TestAppContext,
    save_strategy: SaveStrategy,
) -> (Fixture, &mut gpui::VisualTestContext) {
    cx.update(|cx| {
        let settings_store = settings::SettingsStore::test(cx);
        cx.set_global(settings_store);
        theme_settings::init(theme::LoadThemes::JustBase, cx);
        register_serializable_item::<TestItem>(cx);
    });
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/root", json!({ "file.txt": "dirty" }))
        .await;
    let project = Project::test(fs.clone(), ["/root".as_ref()], cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));

    let item = add_test_item(&workspace, "file.txt", true, cx);

    let template = TaskTemplate {
        label: "test".to_string(),
        command: "echo".to_string(),
        save: save_strategy,
        ..Default::default()
    };
    let task = template
        .resolve_task("test", &task::TaskContext::default())
        .unwrap();
    let dirty_before_spawn: Arc<Mutex<Option<bool>>> = Arc::default();
    let terminal_provider = Box::new(TestTerminalProvider {
        item: item.clone(),
        dirty_before_spawn: dirty_before_spawn.clone(),
    });
    workspace.update(cx, |workspace, _| {
        workspace.terminal_provider = Some(terminal_provider);
    });
    let fixture = Fixture {
        workspace,
        item,
        task,
        dirty_before_spawn,
    };
    (fixture, cx)
}

fn add_test_item(
    workspace: &Entity<Workspace>,
    name: &str,
    active: bool,
    cx: &mut gpui::VisualTestContext,
) -> Entity<TestItem> {
    let item = cx.new(|cx| {
        TestItem::new(cx)
            .with_dirty(true)
            .with_project_items(&[TestProjectItem::new(1, name, cx)])
    });
    workspace.update_in(cx, |workspace, window, cx| {
        let pane = workspace.active_pane().clone();
        workspace.add_item(pane, Box::new(item.clone()), None, true, active, window, cx);
    });
    item
}

#[gpui::test]
async fn test_save_for_task_all(cx: &mut TestAppContext) {
    let (fixture, cx) = create_fixture(cx, SaveStrategy::All).await;
    let workspace = fixture.workspace.downgrade();
    cx.run_until_parked();

    assert!(cx.read(|cx| fixture.item.read(cx).is_dirty));
    fixture.workspace.update_in(cx, |_workspace, window, cx| {
        cx.spawn_in(window, {
            let workspace = workspace.clone();
            async move |_this, cx| {
                Workspace::save_for_task(&workspace, SaveStrategy::All, cx).await;
            }
        })
        .detach();
    });
    cx.run_until_parked();
    assert!(cx.read(|cx| !fixture.item.read(cx).is_dirty));
}

#[gpui::test]
async fn test_save_for_task_none(cx: &mut TestAppContext) {
    let (fixture, cx) = create_fixture(cx, SaveStrategy::None).await;
    let workspace = fixture.workspace.downgrade();
    cx.run_until_parked();

    assert!(cx.read(|cx| fixture.item.read(cx).is_dirty));
    fixture.workspace.update_in(cx, |_workspace, window, cx| {
        cx.spawn_in(window, {
            let workspace = workspace.clone();
            async move |_this, cx| {
                Workspace::save_for_task(&workspace, SaveStrategy::None, cx).await;
            }
        })
        .detach();
    });
    cx.run_until_parked();
    assert!(cx.read(|cx| fixture.item.read(cx).is_dirty));
}

#[gpui::test]
async fn test_save_for_task_current(cx: &mut TestAppContext) {
    let (fixture, cx) = create_fixture(cx, SaveStrategy::Current).await;
    let inactive = add_test_item(&fixture.workspace, "file2.txt", false, cx);
    let workspace = fixture.workspace.downgrade();
    cx.run_until_parked();

    assert!(cx.read(|cx| fixture.item.read(cx).is_dirty));
    assert!(cx.read(|cx| inactive.read(cx).is_dirty));
    fixture.workspace.update_in(cx, |_workspace, window, cx| {
        cx.spawn_in(window, {
            let workspace = workspace.clone();
            async move |_this, cx| {
                Workspace::save_for_task(&workspace, SaveStrategy::Current, cx).await;
            }
        })
        .detach();
    });
    cx.run_until_parked();
    assert!(cx.read(|cx| !fixture.item.read(cx).is_dirty));
    assert!(cx.read(|cx| inactive.read(cx).is_dirty));
}

struct TestTerminalProvider {
    item: Entity<TestItem>,
    dirty_before_spawn: Arc<Mutex<Option<bool>>>,
}

impl TerminalProvider for TestTerminalProvider {
    fn spawn(
        &self,
        _task: task::SpawnInTerminal,
        _window: &mut ui::Window,
        cx: &mut App,
    ) -> Task<Option<Result<ExitStatus>>> {
        *self.dirty_before_spawn.lock() = Some(cx.read_entity(&self.item, |e, _| e.is_dirty));
        Task::ready(Some(Ok(ExitStatus::default())))
    }
}
