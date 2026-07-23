use super::*;

#[test]
fn test_nondeterministic_wake_detection() {
    let config = TestSchedulerConfig {
        allow_parking: false,
        ..Default::default()
    };
    let scheduler = Arc::new(TestScheduler::new(config));

    // A future that captures its waker and sends it to an external thread
    struct SendWakerToThread {
        waker_tx: Option<std::sync::mpsc::Sender<Waker>>,
    }

    impl Future for SendWakerToThread {
        type Output = ();

        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            if let Some(tx) = self.waker_tx.take() {
                tx.send(cx.waker().clone()).ok();
            }
            Poll::Ready(())
        }
    }

    let (waker_tx, waker_rx) = std::sync::mpsc::channel::<Waker>();

    // Get a waker by running a future that sends it
    scheduler.foreground().block_on(SendWakerToThread {
        waker_tx: Some(waker_tx),
    });

    // Spawn a real OS thread that will call wake() on the waker
    let handle = std::thread::spawn(move || {
        if let Ok(waker) = waker_rx.recv() {
            // This should trigger the non-determinism detection
            waker.wake();
        }
    });

    // Wait for the spawned thread to complete
    handle.join().ok();

    // The non-determinism error should be detected when end_test is called
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        scheduler.end_test();
    }));
    assert!(result.is_err(), "Expected end_test to panic");
    let panic_payload = result.unwrap_err();
    let panic_message = panic_payload
        .downcast_ref::<String>()
        .map(|s| s.as_str())
        .or_else(|| panic_payload.downcast_ref::<&str>().copied())
        .unwrap_or("<unknown panic>");
    assert!(
        panic_message.contains("Your test is not deterministic"),
        "Expected panic message to contain non-determinism error, got: {}",
        panic_message
    );
}

#[test]
fn test_nondeterministic_wake_allowed_with_parking() {
    let config = TestSchedulerConfig {
        allow_parking: true,
        ..Default::default()
    };
    let scheduler = Arc::new(TestScheduler::new(config));

    // A future that captures its waker and sends it to an external thread
    struct WakeFromExternalThread {
        waker_sent: bool,
        waker_tx: Option<std::sync::mpsc::Sender<Waker>>,
    }

    impl Future for WakeFromExternalThread {
        type Output = ();

        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            if !self.waker_sent {
                self.waker_sent = true;
                if let Some(tx) = self.waker_tx.take() {
                    tx.send(cx.waker().clone()).ok();
                }
                Poll::Pending
            } else {
                Poll::Ready(())
            }
        }
    }

    let (waker_tx, waker_rx) = std::sync::mpsc::channel::<Waker>();

    // Spawn a real OS thread that will call wake() on the waker
    std::thread::spawn(move || {
        if let Ok(waker) = waker_rx.recv() {
            // With allow_parking, this should NOT panic
            waker.wake();
        }
    });

    // This should complete without panicking
    scheduler.foreground().block_on(WakeFromExternalThread {
        waker_sent: false,
        waker_tx: Some(waker_tx),
    });
}

#[test]
fn test_nondeterministic_waker_drop_detection() {
    let config = TestSchedulerConfig {
        allow_parking: false,
        ..Default::default()
    };
    let scheduler = Arc::new(TestScheduler::new(config));

    // A future that captures its waker and sends it to an external thread
    struct SendWakerToThread {
        waker_tx: Option<std::sync::mpsc::Sender<Waker>>,
    }

    impl Future for SendWakerToThread {
        type Output = ();

        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            if let Some(tx) = self.waker_tx.take() {
                tx.send(cx.waker().clone()).ok();
            }
            Poll::Ready(())
        }
    }

    let (waker_tx, waker_rx) = std::sync::mpsc::channel::<Waker>();

    // Get a waker by running a future that sends it
    scheduler.foreground().block_on(SendWakerToThread {
        waker_tx: Some(waker_tx),
    });

    // Spawn a real OS thread that will drop the waker without calling wake
    let handle = std::thread::spawn(move || {
        if let Ok(waker) = waker_rx.recv() {
            // This should trigger the non-determinism detection on drop
            drop(waker);
        }
    });

    // Wait for the spawned thread to complete
    handle.join().ok();

    // The non-determinism error should be detected when end_test is called
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        scheduler.end_test();
    }));
    assert!(result.is_err(), "Expected end_test to panic");
    let panic_payload = result.unwrap_err();
    let panic_message = panic_payload
        .downcast_ref::<String>()
        .map(|s| s.as_str())
        .or_else(|| panic_payload.downcast_ref::<&str>().copied())
        .unwrap_or("<unknown panic>");
    assert!(
        panic_message.contains("Your test is not deterministic"),
        "Expected panic message to contain non-determinism error, got: {}",
        panic_message
    );
}
