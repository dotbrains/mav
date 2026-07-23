use super::test_support::Yield;
use super::*;

#[test]
fn test_block() {
    let scheduler = Arc::new(TestScheduler::new(TestSchedulerConfig::default()));
    let (tx, rx) = oneshot::channel();

    // Spawn background task to send value
    let _ = scheduler
        .background()
        .spawn(async move {
            tx.send(42).unwrap();
        })
        .detach();

    // Block on receiving the value
    let result = scheduler.foreground().block_on(async { rx.await.unwrap() });
    assert_eq!(result, 42);
}

#[test]
#[should_panic(expected = "Parking forbidden.")]
fn test_parking_panics() {
    let config = TestSchedulerConfig {
        capture_pending_traces: true,
        ..Default::default()
    };
    let scheduler = Arc::new(TestScheduler::new(config));
    scheduler.foreground().block_on(async {
        let (_tx, rx) = oneshot::channel::<()>();
        rx.await.unwrap(); // This will never complete
    });
}

#[test]
fn test_block_with_parking() {
    let config = TestSchedulerConfig {
        allow_parking: true,
        ..Default::default()
    };
    let scheduler = Arc::new(TestScheduler::new(config));
    let (tx, rx) = oneshot::channel();

    // Spawn background task to send value
    let _ = scheduler
        .background()
        .spawn(async move {
            tx.send(42).unwrap();
        })
        .detach();

    // Block on receiving the value (will park if needed)
    let result = scheduler.foreground().block_on(async { rx.await.unwrap() });
    assert_eq!(result, 42);
}

#[test]
fn test_helper_methods() {
    // Test the once method
    let result = TestScheduler::once(async |scheduler: Arc<TestScheduler>| {
        let background = scheduler.background();
        background.spawn(async { 42 }).await
    });
    assert_eq!(result, 42);

    // Test the many method
    let results = TestScheduler::many(3, async |scheduler: Arc<TestScheduler>| {
        let background = scheduler.background();
        background.spawn(async { 10 }).await
    });
    assert_eq!(results, vec![10, 10, 10]);
}

#[test]
fn test_many_with_arbitrary_seed() {
    for seed in [0u64, 1, 5, 42] {
        let mut seeds_seen = Vec::new();
        let iterations = 3usize;

        for current_seed in seed..seed + iterations as u64 {
            let scheduler = Arc::new(TestScheduler::new(TestSchedulerConfig::with_seed(
                current_seed,
            )));
            let captured_seed = current_seed;
            scheduler
                .foreground()
                .block_on(async { seeds_seen.push(captured_seed) });
            scheduler.run();
        }

        assert_eq!(
            seeds_seen,
            (seed..seed + iterations as u64).collect::<Vec<_>>(),
            "Expected {iterations} iterations starting at seed {seed}"
        );
    }
}

#[test]
fn test_block_with_timeout() {
    // Test case: future completes within timeout
    TestScheduler::once(async |scheduler| {
        let foreground = scheduler.foreground();
        let future = future::ready(42);
        let output = foreground.block_with_timeout(Duration::from_millis(100), future);
        assert_eq!(output.ok(), Some(42));
    });

    // Test case: future times out
    TestScheduler::once(async |scheduler| {
        // Make timeout behavior deterministic by forcing the timeout tick budget to be exactly 0.
        // This prevents `block_with_timeout` from making progress via extra scheduler stepping and
        // accidentally completing work that we expect to time out.
        scheduler.set_timeout_ticks(0..=0);

        let foreground = scheduler.foreground();
        let future = future::pending::<()>();
        let output = foreground.block_with_timeout(Duration::from_millis(50), future);
        assert!(output.is_err(), "future should not have finished");
    });

    // Test case: future makes progress via timer but still times out
    let mut results = BTreeSet::new();
    TestScheduler::many(if cfg!(miri) { 5 } else { 100 }, async |scheduler| {
        // Keep the existing probabilistic behavior here (do not force 0 ticks), since this subtest
        // is explicitly checking that some seeds/timeouts can complete while others can time out.
        let task = scheduler.background().spawn(async move {
            Yield { polls: 10 }.await;
            42
        });
        let output = scheduler
            .foreground()
            .block_with_timeout(Duration::from_millis(50), task);
        results.insert(output.ok());
    });
    assert_eq!(
        results.into_iter().collect::<Vec<_>>(),
        if cfg!(miri) {
            vec![Some(42)]
        } else {
            vec![None, Some(42)]
        }
    );

    // Regression test:
    // A timed-out future must not be cancelled. The returned future should still be
    // pollable to completion later. We also want to ensure time only advances when we
    // explicitly advance it (not by yielding).
    TestScheduler::once(async |scheduler| {
        // Force immediate timeout: the timeout tick budget is 0 so we will not step or
        // advance timers inside `block_with_timeout`.
        scheduler.set_timeout_ticks(0..=0);

        let background = scheduler.background();

        // This task should only complete once time is explicitly advanced.
        let task = background.spawn({
            let scheduler = scheduler.clone();
            async move {
                scheduler.timer(Duration::from_millis(100)).await;
                123
            }
        });

        // This should time out before we advance time enough for the timer to fire.
        let timed_out = scheduler
            .foreground()
            .block_with_timeout(Duration::from_millis(50), task);
        assert!(
            timed_out.is_err(),
            "expected timeout before advancing the clock enough for the timer"
        );

        // Now explicitly advance time and ensure the returned future can complete.
        let mut task = timed_out.err().unwrap();
        scheduler.advance_clock(Duration::from_millis(100));
        scheduler.run();

        let output = scheduler.foreground().block_on(&mut task);
        assert_eq!(output, 123);
    });
}

// When calling block, we shouldn't make progress on foreground-spawned futures with the same session id.
#[test]
fn test_block_does_not_progress_same_session_foreground() {
    let mut task2_made_progress_once = false;
    TestScheduler::many(if cfg!(miri) { 5 } else { 1000 }, async |scheduler| {
        let foreground1 = scheduler.foreground();
        let foreground2 = scheduler.foreground();

        let task1 = foreground1.spawn(async move {});
        let task2 = foreground2.spawn(async move {});

        foreground1.block_on(async {
            scheduler.yield_random().await;
            assert!(!task1.is_ready());
            task2_made_progress_once |= task2.is_ready();
        });

        task1.await;
        task2.await;
    });

    assert!(
        task2_made_progress_once,
        "Expected task from different foreground executor to make progress (at least once)"
    );
}
