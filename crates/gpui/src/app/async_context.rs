mod window_context;

use crate::{
    AnyView, AnyWindowHandle, App, AppCell, AppContext, BackgroundExecutor, BorrowAppContext,
    Entity, EntityId, EventEmitter, ForegroundExecutor, Global, GpuiBorrow, Render, Reservation,
    Result, Subscription, Task, Window, WindowHandle,
};
use anyhow::{Context as _, bail};
use futures::future::FutureExt;
use std::{future::Future, rc::Weak};

use super::{Context, WeakEntity};
pub use window_context::AsyncWindowContext;

/// An async-friendly version of [App] with a static lifetime so it can be held across `await` points in async code.
/// You're provided with an instance when calling [App::spawn], and you can also create one with [App::to_async].
///
/// Internally, this holds a weak reference to an `App`. Methods will panic if the app has been dropped,
/// but this should not happen in practice when using foreground tasks spawned via `cx.spawn()`,
/// as the executor checks if the app is alive before running each task.
#[derive(Clone)]
pub struct AsyncApp {
    pub(crate) app: Weak<AppCell>,
    pub(crate) background_executor: BackgroundExecutor,
    pub(crate) foreground_executor: ForegroundExecutor,
}

impl AsyncApp {
    fn app(&self) -> std::rc::Rc<AppCell> {
        self.app
            .upgrade()
            .expect("app was released before async operation completed")
    }
}

impl AppContext for AsyncApp {
    fn new<T: 'static>(&mut self, build_entity: impl FnOnce(&mut Context<T>) -> T) -> Entity<T> {
        let app = self.app();
        let mut app = app.borrow_mut();
        app.new(build_entity)
    }

    fn reserve_entity<T: 'static>(&mut self) -> Reservation<T> {
        let app = self.app();
        let mut app = app.borrow_mut();
        app.reserve_entity()
    }

    fn insert_entity<T: 'static>(
        &mut self,
        reservation: Reservation<T>,
        build_entity: impl FnOnce(&mut Context<T>) -> T,
    ) -> Entity<T> {
        let app = self.app();
        let mut app = app.borrow_mut();
        app.insert_entity(reservation, build_entity)
    }

    fn update_entity<T: 'static, R>(
        &mut self,
        handle: &Entity<T>,
        update: impl FnOnce(&mut T, &mut Context<T>) -> R,
    ) -> R {
        let app = self.app();
        let mut app = app.borrow_mut();
        app.update_entity(handle, update)
    }

    fn as_mut<'a, T>(&'a mut self, _handle: &Entity<T>) -> GpuiBorrow<'a, T>
    where
        T: 'static,
    {
        panic!("Cannot as_mut with an async context. Try calling update() first")
    }

    fn read_entity<T, R>(&self, handle: &Entity<T>, callback: impl FnOnce(&T, &App) -> R) -> R
    where
        T: 'static,
    {
        let app = self.app();
        let lock = app.borrow();
        lock.read_entity(handle, callback)
    }

    fn update_window<T, F>(&mut self, window: AnyWindowHandle, f: F) -> Result<T>
    where
        F: FnOnce(AnyView, &mut Window, &mut App) -> T,
    {
        let app = self.app.upgrade().context("app was released")?;
        let mut lock = app.try_borrow_mut()?;
        if lock.quitting {
            bail!("app is quitting");
        }
        lock.update_window(window, f)
    }

    fn with_window<R>(
        &mut self,
        entity_id: EntityId,
        f: impl FnOnce(&mut Window, &mut App) -> R,
    ) -> Option<R> {
        let app = self.app.upgrade()?;
        let mut lock = app.try_borrow_mut().ok()?;
        if lock.quitting {
            return None;
        }
        lock.with_window(entity_id, f)
    }

    fn read_window<T, R>(
        &self,
        window: &WindowHandle<T>,
        read: impl FnOnce(Entity<T>, &App) -> R,
    ) -> Result<R>
    where
        T: 'static,
    {
        let app = self.app.upgrade().context("app was released")?;
        let lock = app.borrow();
        if lock.quitting {
            bail!("app is quitting");
        }
        lock.read_window(window, read)
    }

    #[track_caller]
    fn background_spawn<R>(&self, future: impl Future<Output = R> + Send + 'static) -> Task<R>
    where
        R: Send + 'static,
    {
        self.background_executor.spawn(future)
    }

    fn read_global<G, R>(&self, callback: impl FnOnce(&G, &App) -> R) -> R
    where
        G: Global,
    {
        let app = self.app();
        let mut lock = app.borrow_mut();
        lock.update(|this| this.read_global(callback))
    }
}

impl AsyncApp {
    /// Schedules all windows in the application to be redrawn.
    pub fn refresh(&self) {
        let app = self.app();
        let mut lock = app.borrow_mut();
        lock.refresh_windows();
    }

    /// Get an executor which can be used to spawn futures in the background.
    pub fn background_executor(&self) -> &BackgroundExecutor {
        &self.background_executor
    }

    /// Get an executor which can be used to spawn futures in the foreground.
    pub fn foreground_executor(&self) -> &ForegroundExecutor {
        &self.foreground_executor
    }

    /// Invoke the given function in the context of the app, then flush any effects produced during its invocation.
    pub fn update<R>(&self, f: impl FnOnce(&mut App) -> R) -> R {
        let app = self.app();
        let mut lock = app.borrow_mut();
        lock.update(f)
    }

    /// Arrange for the given callback to be invoked whenever the given entity emits an event of a given type.
    /// The callback is provided a handle to the emitting entity and a reference to the emitted event.
    pub fn subscribe<T, Event>(
        &mut self,
        entity: &Entity<T>,
        on_event: impl FnMut(Entity<T>, &Event, &mut App) + 'static,
    ) -> Subscription
    where
        T: 'static + EventEmitter<Event>,
        Event: 'static,
    {
        let app = self.app();
        let mut lock = app.borrow_mut();
        lock.subscribe(entity, on_event)
    }

    /// Open a window with the given options based on the root view returned by the given function.
    pub fn open_window<V>(
        &self,
        options: crate::WindowOptions,
        build_root_view: impl FnOnce(&mut Window, &mut App) -> Entity<V>,
    ) -> Result<WindowHandle<V>>
    where
        V: 'static + Render,
    {
        let app = self.app();
        let mut lock = app.borrow_mut();
        if lock.quitting {
            bail!("app is quitting");
        }
        lock.open_window(options, build_root_view)
    }

    /// Schedule a future to be polled in the foreground.
    #[track_caller]
    pub fn spawn<AsyncFn, R>(&self, f: AsyncFn) -> Task<R>
    where
        AsyncFn: AsyncFnOnce(&mut AsyncApp) -> R + 'static,
        R: 'static,
    {
        let mut cx = self.clone();
        self.foreground_executor
            .spawn(async move { f(&mut cx).await }.boxed_local())
    }

    /// Determine whether global state of the specified type has been assigned.
    pub fn has_global<G: Global>(&self) -> bool {
        let app = self.app();
        let app = app.borrow_mut();
        app.has_global::<G>()
    }

    /// Reads the global state of the specified type, passing it to the given callback.
    ///
    /// Panics if no global state of the specified type has been assigned.
    pub fn read_global<G: Global, R>(&self, read: impl FnOnce(&G, &App) -> R) -> R {
        let app = self.app();
        let app = app.borrow_mut();
        read(app.global(), &app)
    }

    /// Reads the global state of the specified type, passing it to the given callback.
    ///
    /// Similar to [`AsyncApp::read_global`], but returns an error instead of panicking
    pub fn try_read_global<G: Global, R>(&self, read: impl FnOnce(&G, &App) -> R) -> Option<R> {
        let app = self.app();
        let app = app.borrow_mut();
        if app.quitting {
            return None;
        }
        Some(read(app.try_global()?, &app))
    }

    /// Reads the global state of the specified type, passing it to the given callback.
    /// A default value is assigned if a global of this type has not yet been assigned.
    pub fn read_default_global<G: Global + Default, R>(
        &self,
        read: impl FnOnce(&G, &App) -> R,
    ) -> R {
        let app = self.app();
        let mut app = app.borrow_mut();
        app.update(|cx| {
            cx.default_global::<G>();
        });
        read(app.global(), &app)
    }

    /// A convenience method for [`App::update_global`](BorrowAppContext::update_global)
    /// for updating the global state of the specified type.
    pub fn update_global<G: Global, R>(&self, update: impl FnOnce(&mut G, &mut App) -> R) -> R {
        let app = self.app();
        let mut app = app.borrow_mut();
        app.update(|cx| cx.update_global(update))
    }

    /// Run something using this entity and cx, when the returned struct is dropped
    pub fn on_drop<T: 'static, Callback: FnOnce(&mut T, &mut Context<T>) + 'static>(
        &self,
        entity: &WeakEntity<T>,
        f: Callback,
    ) -> gpui_util::Deferred<impl FnOnce() + use<T, Callback>> {
        let entity = entity.clone();
        let mut cx = self.clone();
        gpui_util::defer(move || {
            entity.update(&mut cx, f).ok();
        })
    }
}
