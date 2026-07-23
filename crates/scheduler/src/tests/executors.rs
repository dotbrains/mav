use super::*;

#[test]
fn test_foreground_executor_spawn() {
    let result = TestScheduler::once(async |scheduler| {
        let task = scheduler.foreground().spawn(async move { 42 });
        task.await
    });
    assert_eq!(result, 42);
}

#[test]
fn test_background_executor_spawn() {
    TestScheduler::once(async |scheduler| {
        let task = scheduler.background().spawn(async move { 42 });
        let result = task.await;
        assert_eq!(result, 42);
    });
}

#[test]
fn test_scheduler_drops_with_stalled_detached_foreground_task() {
    let scheduler = Arc::new(TestScheduler::new(TestSchedulerConfig::default()));
    let weak_scheduler = Arc::downgrade(&scheduler);
    let (sender, receiver) = oneshot::channel::<()>();

    scheduler
        .foreground()
        .spawn(async move {
            receiver.await.ok();
        })
        .detach();
    scheduler.run();

    drop(scheduler);
    assert!(weak_scheduler.upgrade().is_none());
    drop(sender);
}

#[test]
fn test_scheduler_drops_with_stalled_detached_background_task() {
    let scheduler = Arc::new(TestScheduler::new(TestSchedulerConfig::default()));
    let weak_scheduler = Arc::downgrade(&scheduler);
    let (sender, receiver) = oneshot::channel::<()>();

    scheduler
        .background()
        .spawn(async move {
            receiver.await.ok();
        })
        .detach();
    scheduler.run();

    drop(scheduler);
    assert!(weak_scheduler.upgrade().is_none());
    drop(sender);
}

/// A dedicated task that is never polled must not keep the scheduler alive:
/// its runnable sits in the scheduler's own queue, so any strong scheduler
/// handle captured by the future would form a reference cycle and leak both.
#[test]
fn test_scheduler_drops_with_never_polled_dedicated_task() {
    let scheduler = Arc::new(TestScheduler::new(TestSchedulerConfig::default()));
    let weak_scheduler = Arc::downgrade(&scheduler);

    scheduler
        .background()
        .spawn_dedicated(|_executor| async move {})
        .detach();

    drop(scheduler);
    assert!(weak_scheduler.upgrade().is_none());
}

#[test]
fn test_foreground_task_can_hold_mut_borrow_across_await() {
    TestScheduler::once(async |scheduler| {
        let foreground = scheduler.foreground();
        let (sender, mut receiver) = mpsc::unbounded::<()>();

        foreground
            .spawn(async move {
                receiver.next().await;
            })
            .detach();

        scheduler.run();
        sender.unbounded_send(()).unwrap();
        scheduler.run();
    });
}

#[test]
fn test_send_from_bg_to_fg() {
    TestScheduler::once(async |scheduler| {
        let foreground = scheduler.foreground();
        let background = scheduler.background();

        let (sender, receiver) = oneshot::channel::<i32>();

        background
            .spawn(async move {
                sender.send(42).unwrap();
            })
            .detach();

        let task = foreground.spawn(async move { receiver.await.unwrap() });
        let result = task.await;
        assert_eq!(result, 42);
    });
}
