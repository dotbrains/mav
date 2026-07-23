use super::*;

#[test]
fn test_foreground_ordering() {
    let mut traces = HashSet::new();

    TestScheduler::many(if cfg!(miri) { 5 } else { 100 }, async |scheduler| {
        #[derive(Hash, PartialEq, Eq)]
        struct TraceEntry {
            session: usize,
            task: usize,
        }

        let trace = Rc::new(RefCell::new(Vec::new()));

        let foreground_1 = scheduler.foreground();
        for task in 0..10 {
            foreground_1
                .spawn({
                    let trace = trace.clone();
                    async move {
                        trace.borrow_mut().push(TraceEntry { session: 0, task });
                    }
                })
                .detach();
        }

        let foreground_2 = scheduler.foreground();
        for task in 0..10 {
            foreground_2
                .spawn({
                    let trace = trace.clone();
                    async move {
                        trace.borrow_mut().push(TraceEntry { session: 1, task });
                    }
                })
                .detach();
        }

        scheduler.run();

        assert_eq!(
            trace
                .borrow()
                .iter()
                .filter(|entry| entry.session == 0)
                .map(|entry| entry.task)
                .collect::<Vec<_>>(),
            (0..10).collect::<Vec<_>>()
        );
        assert_eq!(
            trace
                .borrow()
                .iter()
                .filter(|entry| entry.session == 1)
                .map(|entry| entry.task)
                .collect::<Vec<_>>(),
            (0..10).collect::<Vec<_>>()
        );

        traces.insert(trace.take());
    });

    assert!(traces.len() > 1, "Expected at least two traces");
}

#[test]
fn test_timer_ordering() {
    TestScheduler::many(1, async |scheduler| {
        let background = scheduler.background();
        let futures = FuturesUnordered::new();
        futures.push(
            async {
                background.timer(Duration::from_millis(100)).await;
                2
            }
            .boxed(),
        );
        futures.push(
            async {
                background.timer(Duration::from_millis(50)).await;
                1
            }
            .boxed(),
        );
        futures.push(
            async {
                background.timer(Duration::from_millis(150)).await;
                3
            }
            .boxed(),
        );
        assert_eq!(futures.collect::<Vec<_>>().await, vec![1, 2, 3]);
    });
}

#[test]
fn test_randomize_order() {
    // Test deterministic mode: different seeds should produce same execution order
    let mut deterministic_results = HashSet::new();
    for seed in 0..10 {
        let config = TestSchedulerConfig {
            seed,
            randomize_order: false,
            ..Default::default()
        };
        let order = block_on(capture_execution_order(config));
        assert_eq!(order.len(), 6);
        deterministic_results.insert(order);
    }

    // All deterministic runs should produce the same result
    assert_eq!(
        deterministic_results.len(),
        1,
        "Deterministic mode should always produce same execution order"
    );

    // Test randomized mode: different seeds can produce different execution orders
    let mut randomized_results = HashSet::new();
    for seed in 0..20 {
        let config = TestSchedulerConfig::with_seed(seed);
        let order = block_on(capture_execution_order(config));
        assert_eq!(order.len(), 6);
        randomized_results.insert(order);
    }

    // Randomized mode should produce multiple different execution orders
    assert!(
        randomized_results.len() > 1,
        "Randomized mode should produce multiple different orders"
    );
}

async fn capture_execution_order(config: TestSchedulerConfig) -> Vec<String> {
    let scheduler = Arc::new(TestScheduler::new(config));
    let foreground = scheduler.foreground();
    let background = scheduler.background();

    let (sender, receiver) = mpsc::unbounded::<String>();

    // Spawn foreground tasks
    for i in 0..3 {
        let mut sender = sender.clone();
        foreground
            .spawn(async move {
                sender.send(format!("fg-{}", i)).await.ok();
            })
            .detach();
    }

    // Spawn background tasks
    for i in 0..3 {
        let mut sender = sender.clone();
        background
            .spawn(async move {
                sender.send(format!("bg-{}", i)).await.ok();
            })
            .detach();
    }

    drop(sender); // Close sender to signal no more messages
    scheduler.run();

    receiver.collect().await
}

#[test]
fn test_background_priority_scheduling() {
    use parking_lot::Mutex;

    // Run many iterations to get statistical significance
    let mut high_before_low_count = 0;
    let iterations = if cfg!(miri) { 5 } else { 100 };

    for seed in 0..iterations {
        let config = TestSchedulerConfig::with_seed(seed);
        let scheduler = Arc::new(TestScheduler::new(config));
        let background = scheduler.background();

        let execution_order = Arc::new(Mutex::new(Vec::new()));

        // Spawn low priority tasks first
        for i in 0..3 {
            let order = execution_order.clone();
            background
                .spawn_with_priority(Priority::Low, async move {
                    order.lock().push(format!("low-{}", i));
                })
                .detach();
        }

        // Spawn high priority tasks second
        for i in 0..3 {
            let order = execution_order.clone();
            background
                .spawn_with_priority(Priority::High, async move {
                    order.lock().push(format!("high-{}", i));
                })
                .detach();
        }

        scheduler.run();

        // Count how many high priority tasks ran in the first half
        let order = execution_order.lock();
        let high_in_first_half = order
            .iter()
            .take(3)
            .filter(|s| s.starts_with("high"))
            .count();

        if high_in_first_half >= 2 {
            high_before_low_count += 1;
        }
    }

    // High priority tasks should tend to run before low priority tasks
    // With weights of 60 vs 10, high priority should dominate early execution
    assert!(
        high_before_low_count > iterations / 2,
        "Expected high priority tasks to run before low priority tasks more often. \
         Got {} out of {} iterations",
        high_before_low_count,
        iterations
    );
}
