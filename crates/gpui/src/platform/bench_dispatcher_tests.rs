use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use crate::{BackgroundExecutor, BenchDispatcher, ForegroundExecutor};

#[test]
fn run_ready_main_tasks_does_not_wait_for_background_handoffs() {
    let dispatcher = Arc::new(BenchDispatcher::new());
    let background = BackgroundExecutor::new(dispatcher.clone());
    let foreground = ForegroundExecutor::new(dispatcher.clone());

    let (sender, receiver) = futures::channel::oneshot::channel();
    background
        .spawn(async move {
            thread::sleep(Duration::from_millis(10));
            sender.send(()).ok();
        })
        .detach();

    let completed = Arc::new(AtomicBool::new(false));
    foreground
        .spawn({
            let completed = completed.clone();
            async move {
                receiver.await.ok();
                completed.store(true, Ordering::SeqCst);
            }
        })
        .detach();

    assert!(dispatcher.run_ready_main_tasks());
    assert!(!completed.load(Ordering::SeqCst));

    dispatcher.run_until_idle();
    assert!(completed.load(Ordering::SeqCst));
}

#[test]
fn run_until_idle_completes_background_to_main_handoffs() {
    let dispatcher = Arc::new(BenchDispatcher::new());
    let background = BackgroundExecutor::new(dispatcher.clone());
    let foreground = ForegroundExecutor::new(dispatcher.clone());

    let (sender, receiver) = futures::channel::oneshot::channel();
    background
        .spawn(async move {
            thread::sleep(Duration::from_millis(10));
            sender.send(()).ok();
        })
        .detach();

    let completed = Arc::new(AtomicBool::new(false));
    foreground
        .spawn({
            let completed = completed.clone();
            async move {
                receiver.await.ok();
                completed.store(true, Ordering::SeqCst);
            }
        })
        .detach();

    dispatcher.run_until_idle();
    assert!(completed.load(Ordering::SeqCst));
}

#[test]
fn timers_fire_in_real_time() {
    let dispatcher = Arc::new(BenchDispatcher::new());
    let background = BackgroundExecutor::new(dispatcher);

    let fired = Arc::new(AtomicBool::new(false));
    let timer = background.timer(Duration::from_millis(10));
    background
        .spawn({
            let fired = fired.clone();
            async move {
                timer.await;
                fired.store(true, Ordering::SeqCst);
            }
        })
        .detach();

    let deadline = Instant::now() + Duration::from_secs(10);
    while !fired.load(Ordering::SeqCst) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(1));
    }
    assert!(fired.load(Ordering::SeqCst));
}

#[test]
fn cancel_pending_timers_wakes_waiters_without_waiting_for_deadline() {
    let dispatcher = Arc::new(BenchDispatcher::new());
    let background = BackgroundExecutor::new(dispatcher.clone());

    let fired = Arc::new(AtomicBool::new(false));
    let timer = background.timer(Duration::from_secs(10));
    background
        .spawn({
            let fired = fired.clone();
            async move {
                timer.await;
                fired.store(true, Ordering::SeqCst);
            }
        })
        .detach();

    dispatcher.run_until_idle();
    assert_eq!(dispatcher.cancel_pending_timers(), 1);
    dispatcher.run_until_idle();

    assert!(fired.load(Ordering::SeqCst));
    assert_eq!(dispatcher.cancel_pending_timers(), 0);
}
