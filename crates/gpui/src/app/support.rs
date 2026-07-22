use super::*;

/// Contains state associated with an active drag operation, started by dragging an element
/// within the window or by dragging into the app from the underlying platform.
pub struct AnyDrag {
    /// The view used to render this drag
    pub view: AnyView,
    /// The value of the dragged item, to be dropped
    pub value: Arc<dyn Any>,

    /// This is used to render the dragged item in the same place
    /// on the original element that the drag was initiated
    pub cursor_offset: Point<Pixels>,

    /// The cursor style to use while dragging
    pub cursor_style: Option<CursorStyle>,
}

/// Contains state associated with a tooltip. You'll only need this struct if you're implementing
/// tooltip behavior on a custom element. Otherwise, use [Div::tooltip](crate::Interactivity::tooltip).
#[derive(Clone)]
pub struct AnyTooltip {
    /// The view used to display the tooltip
    pub view: AnyView,

    /// The absolute position of the mouse when the tooltip was deployed.
    pub mouse_position: Point<Pixels>,

    /// Given the bounds of the tooltip, checks whether the tooltip should still be visible and
    /// updates its state accordingly. This is needed atop the hovered element's mouse move handler
    /// to handle the case where the element is not painted (e.g. via use of `visible_on_hover`).
    pub check_visible_and_update: Rc<dyn Fn(Bounds<Pixels>, &mut Window, &mut App) -> bool>,
}

/// A keystroke event, and potentially the associated action
#[derive(Debug)]
pub struct KeystrokeEvent {
    /// The keystroke that occurred
    pub keystroke: Keystroke,

    /// The action that was resolved for the keystroke, if any
    pub action: Option<Box<dyn Action>>,

    /// The context stack at the time
    pub context_stack: Vec<KeyContext>,
}

pub(crate) struct NullHttpClient;

impl HttpClient for NullHttpClient {
    fn send(
        &self,
        _req: http_client::Request<http_client::AsyncBody>,
    ) -> futures::future::BoxFuture<
        'static,
        anyhow::Result<http_client::Response<http_client::AsyncBody>>,
    > {
        async move {
            anyhow::bail!("No HttpClient available");
        }
        .boxed()
    }

    fn user_agent(&self) -> Option<&http_client::http::HeaderValue> {
        None
    }

    fn proxy(&self) -> Option<&Url> {
        None
    }
}

/// A mutable reference to an entity owned by GPUI
pub struct GpuiBorrow<'a, T> {
    inner: Option<Lease<T>>,
    app: &'a mut App,
}

impl<'a, T: 'static> GpuiBorrow<'a, T> {
    pub(crate) fn new(inner: Entity<T>, app: &'a mut App) -> Self {
        app.start_update();
        let lease = app.entities.lease(&inner);
        Self {
            inner: Some(lease),
            app,
        }
    }
}

impl<'a, T: 'static> std::borrow::Borrow<T> for GpuiBorrow<'a, T> {
    fn borrow(&self) -> &T {
        self.inner.as_ref().unwrap().borrow()
    }
}

impl<'a, T: 'static> std::borrow::BorrowMut<T> for GpuiBorrow<'a, T> {
    fn borrow_mut(&mut self) -> &mut T {
        self.inner.as_mut().unwrap().borrow_mut()
    }
}

impl<'a, T: 'static> std::ops::Deref for GpuiBorrow<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.inner.as_ref().unwrap()
    }
}

impl<'a, T: 'static> std::ops::DerefMut for GpuiBorrow<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.inner.as_mut().unwrap()
    }
}

impl<'a, T> Drop for GpuiBorrow<'a, T> {
    fn drop(&mut self) {
        let lease = self.inner.take().unwrap();
        self.app.notify(lease.id);
        self.app.entities.end_lease(lease);
        self.app.finish_update();
    }
}
