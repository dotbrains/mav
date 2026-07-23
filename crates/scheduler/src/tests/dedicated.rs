use super::*;

#[test]
fn test_spawn_dedicated_basic_round_trip() {
    let result = TestScheduler::once(async |scheduler| {
        scheduler
            .background()
            .spawn_dedicated(|_executor| async { 42 })
            .await
    });
    assert_eq!(result, 42);
}

#[test]
fn test_spawn_dedicated_not_send_future() {
    let result = TestScheduler::once(async |scheduler| {
        scheduler
            .background()
            .spawn_dedicated(|_executor| async move {
                // `Rc<RefCell<_>>` is `!Send`. If `spawn_dedicated` required
                // the returned future to be `Send`, this wouldn't compile.
                let state = Rc::new(RefCell::new(0_i32));
                for _ in 0..5 {
                    *state.borrow_mut() += 1;
                }
                *state.borrow()
            })
            .await
    });
    assert_eq!(result, 5);
}

#[test]
fn test_spawn_dedicated_send_closure_captures() {
    use parking_lot::Mutex;

    let observed = TestScheduler::once(async |scheduler| {
        let shared = Arc::new(Mutex::new(0_i32));
        let shared_for_closure = shared.clone();
        let returned = scheduler
            .background()
            .spawn_dedicated(move |_executor| {
                // `shared_for_closure` crossed the `Send` boundary of the
                // closure; we then mutate it from inside the !Send future.
                let local = shared_for_closure;
                async move {
                    *local.lock() = 7;
                }
            })
            .await;
        let _: () = returned;
        *shared.lock()
    });
    assert_eq!(observed, 7);
}

#[test]
fn test_spawn_dedicated_inner_spawn_local() {
    let result = TestScheduler::once(async |scheduler| {
        scheduler
            .background()
            .spawn_dedicated(|executor| async move {
                // The provided executor can spawn additional `!Send` work
                // onto the same dedicated session.
                let inner = Rc::new(RefCell::new(0_i32));
                let inner_for_child = inner.clone();
                let child = executor.spawn(async move {
                    *inner_for_child.borrow_mut() = 99;
                    *inner_for_child.borrow()
                });
                child.await
            })
            .await
    });
    assert_eq!(result, 99);
}

#[test]
fn test_spawn_dedicated_determinism_under_many() {
    use parking_lot::Mutex;

    let outcomes = TestScheduler::many(if cfg!(miri) { 4 } else { 20 }, async |scheduler| {
        let trace = Arc::new(Mutex::new(Vec::<u32>::new()));

        let background = scheduler.background();
        let mut tasks = Vec::new();
        for id in 0..4_u32 {
            let trace = trace.clone();
            let task = background.spawn_dedicated(move |executor| async move {
                for step in 0..3 {
                    trace.lock().push(id * 100 + step);
                    executor.spawn(async {}).await;
                }
                id
            });
            tasks.push(task);
        }

        let mut outputs = Vec::new();
        for task in tasks {
            outputs.push(task.await);
        }

        (trace.lock().clone(), outputs)
    });

    // Re-running with the same seed should produce the same trace. Run a
    // second pass with identical seeds and compare to the first.
    let outcomes_replay = TestScheduler::many(if cfg!(miri) { 4 } else { 20 }, async |scheduler| {
        let trace = Arc::new(Mutex::new(Vec::<u32>::new()));

        let background = scheduler.background();
        let mut tasks = Vec::new();
        for id in 0..4_u32 {
            let trace = trace.clone();
            let task = background.spawn_dedicated(move |executor| async move {
                for step in 0..3 {
                    trace.lock().push(id * 100 + step);
                    executor.spawn(async {}).await;
                }
                id
            });
            tasks.push(task);
        }

        let mut outputs = Vec::new();
        for task in tasks {
            outputs.push(task.await);
        }

        (trace.lock().clone(), outputs)
    });

    assert_eq!(
        outcomes, outcomes_replay,
        "per-seed outcomes should be reproducible"
    );

    // Sanity: at least one seed produced a non-monotonic trace,
    // demonstrating that dedicated tasks really do interleave under the
    // scheduler's randomization.
    let any_interleaved = outcomes.iter().any(|(trace, _)| {
        trace
            .windows(2)
            .any(|window| window[0] / 100 != window[1] / 100)
    });
    assert!(
        any_interleaved,
        "expected at least one seed to interleave dedicated tasks"
    );
}

#[test]
fn test_spawn_dedicated_dropping_task_cancels_future() {
    use parking_lot::Mutex;

    let counter_after = TestScheduler::once(async |scheduler| {
        let counter = Arc::new(Mutex::new(0_u32));
        let (resume_tx, resume_rx) = oneshot::channel::<()>();

        let task = {
            let counter = counter.clone();
            scheduler
                .background()
                .spawn_dedicated(move |_executor| async move {
                    *counter.lock() = 1;
                    // Park here until the test resumes us. If the task is
                    // dropped before this resolves, the second assignment
                    // below must never happen.
                    let _ = resume_rx.await;
                    *counter.lock() = 2;
                })
        };

        // Let the dedicated future make its first observable step.
        scheduler.run();
        assert_eq!(*counter.lock(), 1);

        // Cancel by dropping the root task, then unblock the parked oneshot.
        // The future must not advance past the await: counter stays at 1.
        drop(task);
        let _ = resume_tx.send(());
        scheduler.run();

        *counter.lock()
    });

    assert_eq!(
        counter_after, 1,
        "dropping the dedicated task must cancel the root future before its second write"
    );
}

#[test]
fn test_spawn_dedicated_detached_child_runs_after_root_completes() {
    use parking_lot::Mutex;

    let child_ran = TestScheduler::once(async |scheduler| {
        let child_ran = Arc::new(Mutex::new(false));

        let task = {
            let child_ran = child_ran.clone();
            scheduler
                .background()
                .spawn_dedicated(move |executor| async move {
                    executor
                        .spawn(async move {
                            *child_ran.lock() = true;
                        })
                        .detach();
                    // Root returns immediately, before the child has had a
                    // chance to run.
                })
        };

        task.await;

        // Drain the dedicated session. The detached child must run.
        scheduler.run();

        *child_ran.lock()
    });

    assert!(
        child_ran,
        "detached child must complete after the root, not be cancelled with it"
    );
}

// The production smoke test for `spawn_dedicated` lives in the `gpui` crate
// alongside `PlatformScheduler`, which is the real production implementation
// of the `Scheduler` trait. See `crates/gpui/src/platform_scheduler.rs`.
