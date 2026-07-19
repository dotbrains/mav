mod task;

use crate::{Instant, Priority, RunnableMeta, Scheduler, SessionId, Timer};
use async_task::Runnable;
use std::{
    any::Any,
    future::Future,
    marker::PhantomData,
    mem::ManuallyDrop,
    panic::Location,
    pin::Pin,
    rc::Rc,
    sync::Arc,
    task::{Context, Poll},
    thread::{self, ThreadId},
    time::Duration,
};
pub use task::{FallibleTask, Task};

/// A `!Send` executor pinned to a single session. Tasks spawned on it run in
/// order on whichever thread drains the dispatch destination supplied at
/// construction time — typically the main thread for the default session, or
/// a dedicated OS thread for sessions created by `spawn_dedicated_thread`.
#[derive(Clone)]
pub struct LocalExecutor {
    session_id: SessionId,
    scheduler: Arc<dyn Scheduler>,
    // Spawned tasks' schedule callbacks each hold an `Arc` clone of this
    // closure, so the destination it captures stays alive as long as work
    // could still land on it.
    dispatch: Arc<dyn Fn(Runnable<RunnableMeta>) + Send + Sync>,
    not_send: PhantomData<Rc<()>>,
}

impl LocalExecutor {
    /// Constructs a local executor that runs spawned tasks by sending their
    /// runnables through `dispatch`. The `scheduler` is retained for access to
    /// clocks, timers, and other scheduler-level services.
    ///
    /// For the common case of routing runnables through
    /// `Scheduler::schedule_local`, callers pass a closure that does exactly
    /// that. `spawn_dedicated_thread` instead passes a closure that sends to
    /// the dedicated thread's channel.
    pub fn new(
        session_id: SessionId,
        scheduler: Arc<dyn Scheduler>,
        dispatch: impl Fn(Runnable<RunnableMeta>) + Send + Sync + 'static,
    ) -> Self {
        Self {
            session_id,
            scheduler,
            dispatch: Arc::new(dispatch),
            not_send: PhantomData,
        }
    }

    pub fn session_id(&self) -> SessionId {
        self.session_id
    }

    pub fn scheduler(&self) -> &Arc<dyn Scheduler> {
        &self.scheduler
    }

    #[track_caller]
    pub fn spawn<F>(&self, future: F) -> Task<F::Output>
    where
        F: Future + 'static,
        F::Output: 'static,
    {
        let dispatch = self.dispatch.clone();
        let location = Location::caller();
        let (runnable, task) = spawn_local_with_source_location(
            future,
            move |runnable| dispatch(runnable),
            RunnableMeta {
                location,
                spawned: crate::SpawnTime(Instant::now()),
            },
        );
        runnable.schedule();
        Task::from_async_task(task)
    }

    pub fn block_on<Fut: Future>(&self, future: Fut) -> Fut::Output {
        use std::cell::Cell;

        let output = Cell::new(None);
        let future = async {
            output.set(Some(future.await));
        };
        let mut future = std::pin::pin!(future);

        self.scheduler
            .block(Some(self.session_id), future.as_mut(), None);

        output.take().expect("block_on future did not complete")
    }

    /// Block until the future completes or timeout occurs.
    /// Returns Ok(output) if completed, Err(future) if timed out.
    pub fn block_with_timeout<Fut: Future>(
        &self,
        timeout: Duration,
        future: Fut,
    ) -> Result<Fut::Output, impl Future<Output = Fut::Output> + use<Fut>> {
        use std::cell::Cell;

        let output = Cell::new(None);
        let mut future = Box::pin(future);

        {
            let future_ref = &mut future;
            let wrapper = async {
                output.set(Some(future_ref.await));
            };
            let mut wrapper = std::pin::pin!(wrapper);

            self.scheduler
                .block(Some(self.session_id), wrapper.as_mut(), Some(timeout));
        }

        match output.take() {
            Some(value) => Ok(value),
            None => Err(future),
        }
    }

    #[track_caller]
    pub fn timer(&self, duration: Duration) -> Timer {
        self.scheduler.timer(duration)
    }

    pub fn now(&self) -> Instant {
        self.scheduler.clock().now()
    }

    /// Spawn a closure on a fresh session pinned to its own [`LocalExecutor`].
    /// The closure runs on a new OS thread under `PlatformScheduler`, or on
    /// the test scheduler's loop under `TestScheduler`.
    ///
    /// The returned `Task` represents the dedicated work: dropping it cancels
    /// the dedicated closure, `.await`ing it yields the closure's return
    /// value, `.detach()`ing it lets the dedicated work run independently of
    /// the caller.
    #[track_caller]
    pub fn spawn_dedicated<F, Fut>(&self, f: F) -> Task<Fut::Output>
    where
        F: FnOnce(LocalExecutor) -> Fut + Send + 'static,
        Fut: Future + 'static,
        Fut::Output: Send + Sync + 'static,
    {
        self.scheduler
            .clone()
            .spawn_dedicated(box_dedicated(f))
            .downcast::<Fut::Output>()
    }
}

/// Boxes the user-supplied dedicated closure into the type-erased shape
/// expected by [`Scheduler::spawn_dedicated`]. The user's `Fut::Output` is
/// boxed as `Box<dyn Any + Send + Sync>` on the dedicated side and downcast
/// back to `Fut::Output` by [`Task::downcast`] in the wrapper.
fn box_dedicated<F, Fut>(
    f: F,
) -> Box<
    dyn FnOnce(LocalExecutor) -> Pin<Box<dyn Future<Output = Box<dyn Any + Send + Sync>> + 'static>>
        + Send
        + 'static,
>
where
    F: FnOnce(LocalExecutor) -> Fut + Send + 'static,
    Fut: Future + 'static,
    Fut::Output: Send + Sync + 'static,
{
    Box::new(move |executor| {
        Box::pin(async move { Box::new(f(executor).await) as Box<dyn Any + Send + Sync> })
    })
}

#[derive(Clone)]
pub struct BackgroundExecutor {
    scheduler: Arc<dyn Scheduler>,
}

impl BackgroundExecutor {
    pub fn new(scheduler: Arc<dyn Scheduler>) -> Self {
        Self { scheduler }
    }

    #[track_caller]
    pub fn spawn<F>(&self, future: F) -> Task<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.spawn_with_priority(Priority::default(), future)
    }

    #[track_caller]
    pub fn spawn_with_priority<F>(&self, priority: Priority, future: F) -> Task<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        let scheduler = Arc::downgrade(&self.scheduler);
        let location = Location::caller();
        let (runnable, task) = async_task::Builder::new()
            .metadata(RunnableMeta {
                location,
                spawned: crate::SpawnTime(Instant::now()),
            })
            .spawn(
                move |_| future,
                move |runnable| {
                    if let Some(scheduler) = scheduler.upgrade() {
                        scheduler.schedule_background_with_priority(runnable, priority);
                    }
                },
            );
        runnable.schedule();
        Task::from_async_task(task)
    }

    /// Spawns a future on a dedicated realtime thread for audio processing.
    #[track_caller]
    pub fn spawn_realtime<F>(&self, future: F) -> Task<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        let location = Location::caller();
        let (tx, rx) = flume::bounded::<async_task::Runnable<RunnableMeta>>(1);

        self.scheduler.spawn_realtime(Box::new(move || {
            while let Ok(runnable) = rx.recv() {
                runnable.run();
            }
        }));

        let (runnable, task) = async_task::Builder::new()
            .metadata(RunnableMeta {
                location,
                spawned: crate::SpawnTime(Instant::now()),
            })
            .spawn(
                move |_| future,
                move |runnable| {
                    let _ = tx.send(runnable);
                },
            );
        runnable.schedule();
        Task::from_async_task(task)
    }

    #[track_caller]
    pub fn timer(&self, duration: Duration) -> Timer {
        self.scheduler.timer(duration)
    }

    pub fn now(&self) -> Instant {
        self.scheduler.clock().now()
    }

    pub fn scheduler(&self) -> &Arc<dyn Scheduler> {
        &self.scheduler
    }

    /// Spawn a closure on a fresh session pinned to its own [`LocalExecutor`].
    /// The closure runs on a new OS thread under `PlatformScheduler`, or on
    /// the test scheduler's loop under `TestScheduler`.
    ///
    /// The returned `Task` represents the dedicated work: dropping it cancels
    /// the dedicated closure, `.await`ing it yields the closure's return
    /// value, `.detach()`ing it lets the dedicated work run independently of
    /// the caller.
    #[track_caller]
    pub fn spawn_dedicated<F, Fut>(&self, f: F) -> Task<Fut::Output>
    where
        F: FnOnce(LocalExecutor) -> Fut + Send + 'static,
        Fut: Future + 'static,
        Fut::Output: Send + Sync + 'static,
    {
        self.scheduler
            .clone()
            .spawn_dedicated(box_dedicated(f))
            .downcast::<Fut::Output>()
    }
}

/// Variant of `async_task::spawn_local` that includes the source location of the spawn in panics.
#[track_caller]
fn spawn_local_with_source_location<Fut, S>(
    future: Fut,
    schedule: S,
    metadata: RunnableMeta,
) -> (
    async_task::Runnable<RunnableMeta>,
    async_task::Task<Fut::Output, RunnableMeta>,
)
where
    Fut: Future + 'static,
    Fut::Output: 'static,
    S: async_task::Schedule<RunnableMeta> + Send + Sync + 'static,
{
    #[inline]
    fn thread_id() -> ThreadId {
        std::thread_local! {
            static ID: ThreadId = thread::current().id();
        }
        ID.try_with(|id| *id)
            .unwrap_or_else(|_| thread::current().id())
    }

    struct Checked<F> {
        id: ThreadId,
        inner: ManuallyDrop<F>,
        location: &'static Location<'static>,
    }

    impl<F> Drop for Checked<F> {
        fn drop(&mut self) {
            assert_eq!(
                self.id,
                thread_id(),
                "local task dropped by a thread that didn't spawn it. Task spawned at {}",
                self.location
            );
            // SAFETY: `inner` is wrapped in `ManuallyDrop`, so this is the only
            // place it is dropped. The thread check above ensures local futures
            // are dropped on the thread that created them.
            unsafe { ManuallyDrop::drop(&mut self.inner) };
        }
    }

    impl<F: Future> Future for Checked<F> {
        type Output = F::Output;

        fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            // SAFETY: We don't move any fields out of `self`; this mutable
            // reference is only used to check metadata and to project the pin to
            // `inner` below.
            let this = unsafe { self.get_unchecked_mut() };
            assert!(
                this.id == thread_id(),
                "local task polled by a thread that didn't spawn it. Task spawned at {}",
                this.location
            );
            // SAFETY: `inner` is structurally pinned by `Checked`; after
            // `Checked` is pinned, `inner` is never moved. The thread check
            // above ensures the local future is only polled by its spawning
            // thread.
            unsafe { Pin::new_unchecked(&mut *this.inner).poll(cx) }
        }
    }

    let location = metadata.location;

    let future = move |_| Checked {
        id: thread_id(),
        inner: ManuallyDrop::new(future),
        location,
    };

    let builder = async_task::Builder::new().metadata(metadata);
    // SAFETY: `Checked` enforces the invariants required by `spawn_unchecked`:
    // the non-`Send` future is only polled and dropped on the thread that
    // spawned it.
    unsafe { builder.spawn_unchecked(future, schedule) }
}
