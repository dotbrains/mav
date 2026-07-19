use crate::RunnableMeta;
use std::{
    any::Any,
    future::Future,
    marker::PhantomData,
    pin::Pin,
    task::{Context, Poll},
};

/// Task is a primitive that allows work to happen in the background.
///
/// It implements [`Future`] so you can `.await` on it.
///
/// If you drop a task it will be cancelled immediately. Calling [`Task::detach`] allows
/// the task to continue running, but with no way to return a value.
#[must_use]
pub struct Task<T>(TaskState<T>);

enum TaskState<T> {
    /// A task that is ready to return a value
    Ready(Option<T>),

    /// A task that is currently running.
    Spawned(async_task::Task<T, RunnableMeta>),

    /// A typed view of a [`Task<Box<dyn Any + Send + Sync>>`] obtained via
    /// [`Task::downcast`]. The inner task drives the actual work; the
    /// downcast layer just unwraps the `Box<dyn Any + Send + Sync>` on poll.
    Downcast {
        inner: Box<Task<Box<dyn Any + Send + Sync>>>,
        marker: PhantomData<fn() -> T>,
    },
}

impl<T> Task<T> {
    /// Creates a new task that will resolve with the value
    pub fn ready(val: T) -> Self {
        Task(TaskState::Ready(Some(val)))
    }

    /// Creates a Task from an async_task::Task
    pub fn from_async_task(task: async_task::Task<T, RunnableMeta>) -> Self {
        Task(TaskState::Spawned(task))
    }

    pub fn is_ready(&self) -> bool {
        match &self.0 {
            TaskState::Ready(_) => true,
            TaskState::Spawned(task) => task.is_finished(),
            TaskState::Downcast { inner, .. } => inner.is_ready(),
        }
    }

    /// Detaching a task runs it to completion in the background
    pub fn detach(self) {
        match self {
            Task(TaskState::Ready(_)) => {}
            Task(TaskState::Spawned(task)) => task.detach(),
            Task(TaskState::Downcast { inner, .. }) => inner.detach(),
        }
    }

    /// Converts this task into a fallible task that returns `Option<T>`.
    pub fn fallible(self) -> FallibleTask<T> {
        FallibleTask(match self.0 {
            TaskState::Ready(val) => FallibleTaskState::Ready(val),
            TaskState::Spawned(task) => FallibleTaskState::Spawned(task.fallible()),
            TaskState::Downcast { inner, .. } => FallibleTaskState::Downcast {
                inner: Box::new(inner.fallible()),
                marker: PhantomData,
            },
        })
    }
}

impl Task<Box<dyn Any + Send + Sync>> {
    /// Reinterprets the boxed output as a concrete `T` via downcast on
    /// completion. Used by [`super::LocalExecutor::spawn_dedicated`] and
    /// [`super::BackgroundExecutor::spawn_dedicated`] to recover the user closure's
    /// `Fut::Output` from the dyn-safe [`crate::Scheduler::spawn_dedicated`].
    ///
    /// Panics on poll if the inner output is not in fact a `T` -- a logic
    /// error in whatever produced the inner task, since the downcast type is
    /// chosen by the caller of `downcast`.
    pub fn downcast<T: Send + Sync + 'static>(self) -> Task<T> {
        Task(TaskState::Downcast {
            inner: Box::new(self),
            marker: PhantomData,
        })
    }
}

impl<T> std::fmt::Debug for Task<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.0 {
            TaskState::Ready(_) => f.debug_tuple("Task::Ready").finish(),
            TaskState::Spawned(task) => f.debug_tuple("Task::Spawned").field(task).finish(),
            TaskState::Downcast { inner, .. } => {
                f.debug_tuple("Task::Downcast").field(inner).finish()
            }
        }
    }
}

/// A task that returns `Option<T>` instead of panicking when cancelled.
#[must_use]
pub struct FallibleTask<T>(FallibleTaskState<T>);

enum FallibleTaskState<T> {
    /// A task that is ready to return a value
    Ready(Option<T>),

    /// A task that is currently running (wraps async_task::FallibleTask).
    Spawned(async_task::FallibleTask<T, RunnableMeta>),

    /// Mirror of [`TaskState::Downcast`] for fallible tasks.
    Downcast {
        inner: Box<FallibleTask<Box<dyn Any + Send + Sync>>>,
        marker: PhantomData<fn() -> T>,
    },
}

impl<T> FallibleTask<T> {
    /// Creates a new fallible task that will resolve with the value.
    pub fn ready(val: T) -> Self {
        FallibleTask(FallibleTaskState::Ready(Some(val)))
    }

    /// Detaching a task runs it to completion in the background.
    pub fn detach(self) {
        match self.0 {
            FallibleTaskState::Ready(_) => {}
            FallibleTaskState::Spawned(task) => task.detach(),
            FallibleTaskState::Downcast { inner, .. } => inner.detach(),
        }
    }
}

impl<T: 'static> Future for FallibleTask<T> {
    type Output = Option<T>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        match unsafe { self.get_unchecked_mut() } {
            FallibleTask(FallibleTaskState::Ready(val)) => Poll::Ready(val.take()),
            FallibleTask(FallibleTaskState::Spawned(task)) => Pin::new(task).poll(cx),
            FallibleTask(FallibleTaskState::Downcast { inner, .. }) => {
                match Pin::new(inner.as_mut()).poll(cx) {
                    Poll::Ready(Some(boxed_any)) => Poll::Ready(Some(
                        *boxed_any
                            .downcast::<T>()
                            .expect("FallibleTask::poll: downcast type mismatch"),
                    )),
                    Poll::Ready(None) => Poll::Ready(None),
                    Poll::Pending => Poll::Pending,
                }
            }
        }
    }
}

impl<T> std::fmt::Debug for FallibleTask<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.0 {
            FallibleTaskState::Ready(_) => f.debug_tuple("FallibleTask::Ready").finish(),
            FallibleTaskState::Spawned(task) => {
                f.debug_tuple("FallibleTask::Spawned").field(task).finish()
            }
            FallibleTaskState::Downcast { inner, .. } => f
                .debug_tuple("FallibleTask::Downcast")
                .field(inner)
                .finish(),
        }
    }
}

impl<T: 'static> Future for Task<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        match unsafe { self.get_unchecked_mut() } {
            Task(TaskState::Ready(val)) => Poll::Ready(val.take().unwrap()),
            Task(TaskState::Spawned(task)) => Pin::new(task).poll(cx),
            Task(TaskState::Downcast { inner, .. }) => match Pin::new(inner.as_mut()).poll(cx) {
                Poll::Ready(boxed_any) => Poll::Ready(
                    *boxed_any
                        .downcast::<T>()
                        .expect("Task::poll: downcast type mismatch"),
                ),
                Poll::Pending => Poll::Pending,
            },
        }
    }
}
