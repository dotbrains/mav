use super::*;

#[derive(RegisterComponent)]
pub struct MessageNotification {
    pub(super) focus_handle: FocusHandle,
    pub(super) build_content: Box<dyn Fn(&mut Window, &mut Context<Self>) -> AnyElement>,
    pub(super) button_style: Option<ButtonStyle>,
    pub(super) content_icon: Option<IconName>,
    pub(super) content_icon_color: Option<Color>,
    pub(super) secondary_content: Option<SharedString>,
    pub(super) copy_text: Option<SharedString>,
    pub(super) primary_message: Option<SharedString>,
    pub(super) primary_icon: Option<ActionIcon>,
    pub(super) primary_icon_color: Option<Color>,
    pub(super) primary_on_click: Option<Arc<dyn Fn(&mut Window, &mut Context<Self>)>>,
    pub(super) secondary_message: Option<SharedString>,
    pub(super) secondary_icon: Option<ActionIcon>,
    pub(super) secondary_icon_color: Option<Color>,
    pub(super) secondary_on_click: Option<Arc<dyn Fn(&mut Window, &mut Context<Self>)>>,
    pub(super) more_info_message: Option<SharedString>,
    pub(super) more_info_url: Option<Arc<str>>,
    pub(super) show_close_button: bool,
    pub(super) show_suppress_button: bool,
    pub(super) title: Option<SharedString>,
    pub(super) scroll_handle: ScrollHandle,
    pub(super) auto_hide: Option<AutoHideState>,
}

impl Focusable for MessageNotification {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<DismissEvent> for MessageNotification {}
impl EventEmitter<SuppressEvent> for MessageNotification {}

impl Notification for MessageNotification {}

impl FluentBuilder for MessageNotification {}

impl MessageNotification {
    pub fn new<S>(message: S, cx: &mut App) -> MessageNotification
    where
        S: Into<SharedString>,
    {
        let message = message.into();
        Self::new_from_builder(cx, move |_, _| {
            Label::new(message.clone()).into_any_element()
        })
    }

    pub fn new_from_builder<F>(cx: &mut App, content: F) -> MessageNotification
    where
        F: 'static + Fn(&mut Window, &mut Context<Self>) -> AnyElement,
    {
        Self {
            build_content: Box::new(content),
            button_style: None,
            content_icon: None,
            content_icon_color: None,
            secondary_content: None,
            copy_text: None,
            primary_message: None,
            primary_icon: None,
            primary_icon_color: None,
            primary_on_click: None,
            secondary_message: None,
            secondary_icon: None,
            secondary_icon_color: None,
            secondary_on_click: None,
            more_info_message: None,
            more_info_url: None,
            show_close_button: true,
            show_suppress_button: true,
            title: None,
            focus_handle: cx.focus_handle(),
            scroll_handle: ScrollHandle::new(),
            auto_hide: None,
        }
    }

    pub fn button_style(mut self, style: ButtonStyle) -> Self {
        self.button_style = Some(style);
        self
    }

    pub fn primary_message<S>(mut self, message: S) -> Self
    where
        S: Into<SharedString>,
    {
        self.primary_message = Some(message.into());
        self
    }

    /// Show `icon` at the start (left) of the primary action button label.
    pub fn primary_icon(mut self, icon: IconName) -> Self {
        self.primary_icon = Some(ActionIcon::start(icon));
        self
    }

    /// Show `icon` at the end (right) of the primary action button label.
    pub fn primary_end_icon(mut self, icon: IconName) -> Self {
        self.primary_icon = Some(ActionIcon::end(icon));
        self
    }

    pub fn primary_icon_color(mut self, color: Color) -> Self {
        self.primary_icon_color = Some(color);
        self
    }

    pub fn primary_on_click<F>(mut self, on_click: F) -> Self
    where
        F: 'static + Fn(&mut Window, &mut Context<Self>),
    {
        self.primary_on_click = Some(Arc::new(on_click));
        self
    }

    pub fn primary_on_click_arc<F>(mut self, on_click: Arc<F>) -> Self
    where
        F: 'static + Fn(&mut Window, &mut Context<Self>),
    {
        self.primary_on_click = Some(on_click);
        self
    }

    pub fn secondary_message<S>(mut self, message: S) -> Self
    where
        S: Into<SharedString>,
    {
        self.secondary_message = Some(message.into());
        self
    }

    /// Show `icon` at the start (left) of the secondary action button label.
    pub fn secondary_icon(mut self, icon: IconName) -> Self {
        self.secondary_icon = Some(ActionIcon::start(icon));
        self
    }

    /// Show `icon` at the end (right) of the secondary action button label.
    pub fn secondary_end_icon(mut self, icon: IconName) -> Self {
        self.secondary_icon = Some(ActionIcon::end(icon));
        self
    }

    pub fn secondary_icon_color(mut self, color: Color) -> Self {
        self.secondary_icon_color = Some(color);
        self
    }

    pub fn secondary_on_click<F>(mut self, on_click: F) -> Self
    where
        F: 'static + Fn(&mut Window, &mut Context<Self>),
    {
        self.secondary_on_click = Some(Arc::new(on_click));
        self
    }

    pub fn secondary_on_click_arc<F>(mut self, on_click: Arc<F>) -> Self
    where
        F: 'static + Fn(&mut Window, &mut Context<Self>),
    {
        self.secondary_on_click = Some(on_click);
        self
    }

    pub fn more_info_message<S>(mut self, message: S) -> Self
    where
        S: Into<SharedString>,
    {
        self.more_info_message = Some(message.into());
        self
    }

    pub fn more_info_url<S>(mut self, url: S) -> Self
    where
        S: Into<Arc<str>>,
    {
        self.more_info_url = Some(url.into());
        self
    }

    pub fn dismiss(&mut self, cx: &mut Context<Self>) {
        cx.emit(DismissEvent);
    }

    pub fn show_close_button(mut self, show: bool) -> Self {
        self.show_close_button = show;
        self
    }

    /// Determines whether the given notification ID should be suppressible
    /// Suppressed notifications will not be shown anymor
    pub fn show_suppress_button(mut self, show: bool) -> Self {
        self.show_suppress_button = show;
        self
    }

    pub fn with_title<S>(mut self, title: S) -> Self
    where
        S: Into<SharedString>,
    {
        self.title = Some(title.into());
        self
    }

    pub fn content_icon(mut self, icon: IconName, color: Color) -> Self {
        self.content_icon = Some(icon);
        self.content_icon_color = Some(color);
        self
    }

    pub fn secondary_content<S: Into<SharedString>>(mut self, text: S) -> Self {
        self.secondary_content = Some(text.into());
        self
    }

    pub fn copy_text<S: Into<SharedString>>(mut self, text: S) -> Self {
        self.copy_text = Some(text.into());
        self
    }

    fn auto_dismiss(mut self, severity: ErrorSeverity, cx: &mut Context<Self>) -> Self {
        if let Some(delay) = severity.auto_dismiss_delay() {
            self.auto_hide = Some(AutoHideState::new(delay, cx));
        }
        self
    }

    pub fn from_workspace_error<E: WorkspaceError>(error: E, cx: &mut Context<Self>) -> Self {
        let primary_message = error.primary_message();
        let severity = error.severity();
        let primary_action = error.primary_action();
        let secondary_action = error.secondary_action();

        Self::new(primary_message.clone(), cx)
            .content_icon(IconName::Warning, Color::Error)
            .button_style(ButtonStyle::Outlined)
            .copy_text(primary_message)
            .show_suppress_button(false)
            .when_some(error.secondary_message(), |this, text| {
                this.secondary_content(text)
            })
            .map(|this| {
                let ErrorAction {
                    label,
                    icon,
                    tooltip: _,
                    handler,
                } = primary_action;

                this.primary_message(label)
                    .when_some(icon, |this, icon| match icon.position {
                        IconPosition::Start => this.primary_icon(icon.name),
                        IconPosition::End => this.primary_end_icon(icon.name),
                    })
                    .map(|this| match handler {
                        ErrorActionHandler::Action(action) => {
                            this.primary_on_click(move |window, cx| {
                                window.dispatch_action(action.boxed_clone(), cx);
                            })
                        }
                        ErrorActionHandler::Dismiss => {
                            this.primary_on_click(move |_, cx| cx.emit(DismissEvent))
                        }
                    })
            })
            .when_some(secondary_action, |this, action| {
                let ErrorAction {
                    label,
                    icon,
                    tooltip: _,
                    handler,
                } = action;

                this.secondary_message(label)
                    .when_some(icon, |this, icon| match icon.position {
                        IconPosition::Start => this.secondary_icon(icon.name),
                        IconPosition::End => this.secondary_end_icon(icon.name),
                    })
                    .map(|this| match handler {
                        ErrorActionHandler::Action(handler) => {
                            this.secondary_on_click(move |window, cx| {
                                window.dispatch_action(handler.boxed_clone(), cx);
                            })
                        }
                        ErrorActionHandler::Dismiss => {
                            this.secondary_on_click(move |_, cx| cx.emit(DismissEvent))
                        }
                    })
            })
            .auto_dismiss(severity, cx)
    }

    pub(super) fn on_hover_changed(&mut self, hovering: bool, cx: &mut Context<Self>) {
        if let Some(auto_hide) = self.auto_hide.as_mut() {
            auto_hide.set_hovered(hovering, cx);
        }
    }

    pub(super) fn opacity(&self) -> f32 {
        self.auto_hide
            .as_ref()
            .map_or(1.0, |auto_hide| auto_hide.opacity())
    }

    pub(super) fn needs_animation_frame(&self) -> bool {
        self.auto_hide
            .as_ref()
            .is_some_and(|auto_hide| auto_hide.needs_animation_frame())
    }
}
