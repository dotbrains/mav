use super::AsyncApp;
use crate::{
    AnyView, AnyWindowHandle, App, AppContext, BorrowAppContext, Entity, EntityId, Focusable,
    Global, GpuiBorrow, PromptButton, PromptLevel, Render, Reservation, Result, Task,
    VisualContext, Window, WindowHandle,
};
use anyhow::Context as _;
use derive_more::{Deref, DerefMut};
use futures::{channel::oneshot, future::FutureExt};
use std::future::Future;

use crate::app::Context;

/// A cloneable, owned handle to the application context,
/// composed with the window associated with the current task.
#[derive(Clone, Deref, DerefMut)]
pub struct AsyncWindowContext {
    #[deref]
    #[deref_mut]
    app: AsyncApp,
    window: AnyWindowHandle,
}

impl AsyncWindowContext {
    pub(crate) fn new_context(app: AsyncApp, window: AnyWindowHandle) -> Self {
        Self { app, window }
    }

    /// Get the handle of the window this context is associated with.
    pub fn window_handle(&self) -> AnyWindowHandle {
        self.window
    }

    /// A convenience method for [`App::update_window`].
    pub fn update<R>(&mut self, update: impl FnOnce(&mut Window, &mut App) -> R) -> Result<R> {
        self.app
            .update_window(self.window, |_, window, cx| update(window, cx))
    }

    /// A convenience method for [`App::update_window`].
    pub fn update_root<R>(
        &mut self,
        update: impl FnOnce(AnyView, &mut Window, &mut App) -> R,
    ) -> Result<R> {
        self.app.update_window(self.window, update)
    }

    /// A convenience method for [`Window::on_next_frame`].
    pub fn on_next_frame(&mut self, f: impl FnOnce(&mut Window, &mut App) + 'static) {
        self.app
            .update_window(self.window, |_, window, _| window.on_next_frame(f))
            .ok();
    }

    /// A convenience method for [`App::global`].
    pub fn read_global<G: Global, R>(
        &mut self,
        read: impl FnOnce(&G, &Window, &App) -> R,
    ) -> Result<R> {
        self.app
            .update_window(self.window, |_, window, cx| read(cx.global(), window, cx))
    }

    /// A convenience method for [`App::update_global`](BorrowAppContext::update_global).
    /// for updating the global state of the specified type.
    pub fn update_global<G, R>(
        &mut self,
        update: impl FnOnce(&mut G, &mut Window, &mut App) -> R,
    ) -> Result<R>
    where
        G: Global,
    {
        self.app.update_window(self.window, |_, window, cx| {
            cx.update_global(|global, cx| update(global, window, cx))
        })
    }

    /// Schedule a future to be executed on the main thread. This is used for collecting
    /// the results of background tasks and updating the UI.
    #[track_caller]
    pub fn spawn<AsyncFn, R>(&self, f: AsyncFn) -> Task<R>
    where
        AsyncFn: AsyncFnOnce(&mut AsyncWindowContext) -> R + 'static,
        R: 'static,
    {
        let mut cx = self.clone();
        self.foreground_executor
            .spawn(async move { f(&mut cx).await }.boxed_local())
    }

    /// Present a platform dialog.
    /// The provided message will be presented, along with buttons for each answer.
    /// When a button is clicked, the returned Receiver will receive the index of the clicked button.
    pub fn prompt<T>(
        &mut self,
        level: PromptLevel,
        message: &str,
        detail: Option<&str>,
        answers: &[T],
    ) -> oneshot::Receiver<usize>
    where
        T: Clone + Into<PromptButton>,
    {
        self.app
            .update_window(self.window, |_, window, cx| {
                window.prompt(level, message, detail, answers, cx)
            })
            .unwrap_or_else(|_| oneshot::channel().1)
    }
}

impl AppContext for AsyncWindowContext {
    fn new<T>(&mut self, build_entity: impl FnOnce(&mut Context<T>) -> T) -> Entity<T>
    where
        T: 'static,
    {
        let mut build_entity = Some(build_entity);
        match self.app.update_window(self.window, |_, _, cx| {
            cx.new(
                build_entity
                    .take()
                    .expect("build_entity is taken exactly once"),
            )
        }) {
            Ok(entity) => entity,
            Err(_) => self.app.new(
                build_entity
                    .take()
                    .expect("update_window returned Err without invoking the closure"),
            ),
        }
    }

    fn reserve_entity<T: 'static>(&mut self) -> Reservation<T> {
        self.app.reserve_entity()
    }

    fn insert_entity<T: 'static>(
        &mut self,
        reservation: Reservation<T>,
        build_entity: impl FnOnce(&mut Context<T>) -> T,
    ) -> Entity<T> {
        let mut args = Some((reservation, build_entity));
        match self.app.update_window(self.window, |_, _, cx| {
            let (reservation, build_entity) = args.take().expect("args are taken exactly once");
            cx.insert_entity(reservation, build_entity)
        }) {
            Ok(entity) => entity,
            Err(_) => {
                let (reservation, build_entity) = args
                    .take()
                    .expect("update_window returned Err without invoking the closure");
                self.app.insert_entity(reservation, build_entity)
            }
        }
    }

    fn update_entity<T: 'static, R>(
        &mut self,
        handle: &Entity<T>,
        update: impl FnOnce(&mut T, &mut Context<T>) -> R,
    ) -> R {
        self.app.update_entity(handle, update)
    }

    fn as_mut<'a, T>(&'a mut self, _: &Entity<T>) -> GpuiBorrow<'a, T>
    where
        T: 'static,
    {
        panic!("Cannot use as_mut() from an async context, call `update`")
    }

    fn read_entity<T, R>(&self, handle: &Entity<T>, read: impl FnOnce(&T, &App) -> R) -> R
    where
        T: 'static,
    {
        self.app.read_entity(handle, read)
    }

    fn update_window<T, F>(&mut self, window: AnyWindowHandle, update: F) -> Result<T>
    where
        F: FnOnce(AnyView, &mut Window, &mut App) -> T,
    {
        self.app.update_window(window, update)
    }

    fn with_window<R>(
        &mut self,
        entity_id: EntityId,
        f: impl FnOnce(&mut Window, &mut App) -> R,
    ) -> Option<R> {
        self.app.with_window(entity_id, f)
    }

    fn read_window<T, R>(
        &self,
        window: &WindowHandle<T>,
        read: impl FnOnce(Entity<T>, &App) -> R,
    ) -> Result<R>
    where
        T: 'static,
    {
        self.app.read_window(window, read)
    }

    #[track_caller]
    fn background_spawn<R>(&self, future: impl Future<Output = R> + Send + 'static) -> Task<R>
    where
        R: Send + 'static,
    {
        self.app.background_executor.spawn(future)
    }

    fn read_global<G, R>(&self, callback: impl FnOnce(&G, &App) -> R) -> R
    where
        G: Global,
    {
        self.app.read_global(callback)
    }
}

impl VisualContext for AsyncWindowContext {
    type Result<T> = Result<T>;

    fn window_handle(&self) -> AnyWindowHandle {
        self.window
    }

    fn new_window_entity<T: 'static>(
        &mut self,
        build_entity: impl FnOnce(&mut Window, &mut Context<T>) -> T,
    ) -> Result<Entity<T>> {
        self.app.update_window(self.window, |_, window, cx| {
            cx.new(|cx| build_entity(window, cx))
        })
    }

    fn update_window_entity<T: 'static, R>(
        &mut self,
        view: &Entity<T>,
        update: impl FnOnce(&mut T, &mut Window, &mut Context<T>) -> R,
    ) -> Result<R> {
        let view = view.clone();
        self.app
            .with_window(view.entity_id(), |window, app| {
                view.update(app, |entity, cx| update(entity, window, cx))
            })
            .context("entity has no current window")
    }

    fn replace_root_view<V>(
        &mut self,
        build_view: impl FnOnce(&mut Window, &mut Context<V>) -> V,
    ) -> Result<Entity<V>>
    where
        V: 'static + Render,
    {
        self.app.update_window(self.window, |_, window, cx| {
            window.replace_root(cx, build_view)
        })
    }

    fn focus<V>(&mut self, view: &Entity<V>) -> Result<()>
    where
        V: Focusable,
    {
        self.app.update_window(self.window, |_, window, cx| {
            view.read(cx).focus_handle(cx).focus(window, cx);
        })
    }
}
