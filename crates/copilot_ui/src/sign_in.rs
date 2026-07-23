mod code_verification;
mod config_view;

use anyhow::Context as _;
use copilot::{Copilot, GlobalCopilotAuth, Status};
use gpui::{
    App, Context, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable, InteractiveElement,
    IntoElement, MouseDownEvent, ParentElement, Render, Styled, Subscription, TaskExt, Window,
    WindowBounds, WindowOptions, div, point,
};
use ui::{CommonAnimationExt, Vector, VectorName, prelude::*};
use util::ResultExt as _;
use workspace::{AppState, Toast, Workspace, notifications::NotificationId};

const COPILOT_SIGN_UP_URL: &str = "https://github.com/features/copilot";
const ERROR_LABEL: &str =
    "Copilot had issues starting. You can try reinstalling it and signing in again.";

struct CopilotStatusToast;

pub fn initiate_sign_in(copilot: Entity<Copilot>, window: &mut Window, cx: &mut App) {
    let is_reinstall = false;
    initiate_sign_in_impl(copilot, is_reinstall, window, cx)
}

pub fn initiate_sign_out(copilot: Entity<Copilot>, window: &mut Window, cx: &mut App) {
    copilot_toast(Some("Signing out of Copilot…"), window, cx);

    let sign_out_task = copilot.update(cx, |copilot, cx| copilot.sign_out(cx));
    window
        .spawn(cx, async move |cx| match sign_out_task.await {
            Ok(()) => {
                cx.update(|window, cx| copilot_toast(Some("Signed out of Copilot"), window, cx))
            }
            Err(err) => cx.update(|window, cx| {
                if let Some(workspace) = Workspace::for_window(window, cx) {
                    workspace.update(cx, |workspace, cx| {
                        workspace.show_error(format!("Error: {err}"), cx);
                    })
                } else {
                    log::error!("{:?}", err);
                }
            }),
        })
        .detach();
}

pub fn reinstall_and_sign_in(copilot: Entity<Copilot>, window: &mut Window, cx: &mut App) {
    let _ = copilot.update(cx, |copilot, cx| copilot.reinstall(cx));
    let is_reinstall = true;
    initiate_sign_in_impl(copilot, is_reinstall, window, cx);
}

fn open_copilot_code_verification_window(copilot: &Entity<Copilot>, window: &Window, cx: &mut App) {
    let current_window_center = window.bounds().center();
    let height = px(450.);
    let width = px(350.);
    let window_bounds = WindowBounds::Windowed(gpui::bounds(
        current_window_center - point(height / 2.0, width / 2.0),
        gpui::size(height, width),
    ));
    cx.open_window(
        WindowOptions {
            kind: gpui::WindowKind::Floating,
            window_bounds: Some(window_bounds),
            is_resizable: false,
            is_movable: true,
            titlebar: Some(gpui::TitlebarOptions {
                appears_transparent: true,
                ..Default::default()
            }),
            ..Default::default()
        },
        |window, cx| cx.new(|cx| CopilotCodeVerification::new(&copilot, window, cx)),
    )
    .context("Failed to open Copilot code verification window")
    .log_err();
}

fn copilot_toast(message: Option<&'static str>, window: &Window, cx: &mut App) {
    const NOTIFICATION_ID: NotificationId = NotificationId::unique::<CopilotStatusToast>();

    let Some(workspace) = Workspace::for_window(window, cx) else {
        return;
    };

    cx.defer(move |cx| {
        workspace.update(cx, |workspace, cx| match message {
            Some(message) => workspace.show_toast(Toast::new(NOTIFICATION_ID, message), cx),
            None => workspace.dismiss_toast(&NOTIFICATION_ID, cx),
        });
    })
}

pub fn initiate_sign_in_impl(
    copilot: Entity<Copilot>,
    is_reinstall: bool,
    window: &mut Window,
    cx: &mut App,
) {
    if matches!(copilot.read(cx).status(), Status::Disabled) {
        copilot.update(cx, |copilot, cx| copilot.start_copilot(false, true, cx));
    }
    match copilot.read(cx).status() {
        Status::Starting { task } => {
            copilot_toast(
                Some(if is_reinstall {
                    "Copilot is reinstalling…"
                } else {
                    "Copilot is starting…"
                }),
                window,
                cx,
            );

            window
                .spawn(cx, async move |cx| {
                    task.await;
                    cx.update(|window, cx| match copilot.read(cx).status() {
                        Status::Authorized => {
                            copilot_toast(Some("Copilot has started."), window, cx)
                        }
                        _ => {
                            copilot_toast(None, window, cx);
                            copilot
                                .update(cx, |copilot, cx| copilot.sign_in(cx))
                                .detach_and_log_err(cx);
                            open_copilot_code_verification_window(&copilot, window, cx);
                        }
                    })
                    .log_err();
                })
                .detach();
        }
        _ => {
            copilot
                .update(cx, |copilot, cx| copilot.sign_in(cx))
                .detach();
            open_copilot_code_verification_window(&copilot, window, cx);
        }
    }
}

pub struct CopilotCodeVerification {
    status: Status,
    connect_clicked: bool,
    focus_handle: FocusHandle,
    copilot: Entity<Copilot>,
    _subscription: Subscription,
    sign_up_url: Option<String>,
}

impl Focusable for CopilotCodeVerification {
    fn focus_handle(&self, _: &App) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<DismissEvent> for CopilotCodeVerification {}

impl CopilotCodeVerification {
    pub fn new(copilot: &Entity<Copilot>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        window.on_window_should_close(cx, |window, cx| {
            if let Some(this) = window.root::<CopilotCodeVerification>().flatten() {
                this.update(cx, |this, cx| {
                    this.before_dismiss(cx);
                });
            }
            true
        });
        cx.subscribe_in(
            &cx.entity(),
            window,
            |this, _, _: &DismissEvent, window, cx| {
                window.remove_window();
                this.before_dismiss(cx);
            },
        )
        .detach();

        let status = copilot.read(cx).status();
        Self {
            status,
            connect_clicked: false,
            focus_handle: cx.focus_handle(),
            copilot: copilot.clone(),
            sign_up_url: None,
            _subscription: cx.observe(copilot, |this, copilot, cx| {
                let status = copilot.read(cx).status();
                match status {
                    Status::Authorized | Status::Unauthorized | Status::SigningIn { .. } => {
                        this.set_status(status, cx)
                    }
                    _ => cx.emit(DismissEvent),
                }
            }),
        }
    }

    pub fn set_status(&mut self, status: Status, cx: &mut Context<Self>) {
        self.status = status;
        cx.notify();
    }

    fn before_dismiss(
        &mut self,
        cx: &mut Context<'_, CopilotCodeVerification>,
    ) -> workspace::DismissDecision {
        self.copilot.update(cx, |copilot, cx| {
            if matches!(copilot.status(), Status::SigningIn { .. }) {
                copilot.sign_out(cx).detach_and_log_err(cx);
            }
        });
        workspace::DismissDecision::Dismiss(true)
    }
}

impl Render for CopilotCodeVerification {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let prompt = match &self.status {
            Status::SigningIn { prompt: None } => Icon::new(IconName::ArrowCircle)
                .color(Color::Muted)
                .with_rotate_animation(2)
                .into_any_element(),
            Status::SigningIn {
                prompt: Some(prompt),
            } => code_verification::render_prompting_modal(
                self.copilot.clone(),
                self.connect_clicked,
                prompt,
                cx,
            )
            .into_any_element(),
            Status::Unauthorized => {
                self.connect_clicked = false;
                code_verification::render_unauthorized_modal(self.sign_up_url.as_deref(), cx)
                    .into_any_element()
            }
            Status::Authorized => {
                self.connect_clicked = false;
                code_verification::render_enabled_modal(cx).into_any_element()
            }
            Status::Error(..) => {
                code_verification::render_error_modal(self.copilot.clone(), cx).into_any_element()
            }
            _ => div().into_any_element(),
        };

        v_flex()
            .id("copilot_code_verification")
            .track_focus(&self.focus_handle(cx))
            .size_full()
            .px_4()
            .py_8()
            .gap_2()
            .items_center()
            .justify_center()
            .elevation_3(cx)
            .on_action(cx.listener(|_, _: &menu::Cancel, _, cx| {
                cx.emit(DismissEvent);
            }))
            .on_any_mouse_down(cx.listener(|this, _: &MouseDownEvent, window, cx| {
                window.focus(&this.focus_handle, cx);
            }))
            .child(
                Vector::new(VectorName::MavXCopilot, rems(8.), rems(4.))
                    .color(Color::Custom(cx.theme().colors().icon)),
            )
            .child(prompt)
    }
}

pub struct ConfigurationView {
    copilot_status: Option<Status>,
    is_authenticated: Box<dyn Fn(&mut App) -> bool + 'static>,
    edit_prediction: bool,
    _subscription: Option<Subscription>,
}

pub enum ConfigurationMode {
    Chat,
    EditPrediction,
}

impl ConfigurationView {
    pub fn new(
        is_authenticated: impl Fn(&mut App) -> bool + 'static,
        mode: ConfigurationMode,
        cx: &mut Context<Self>,
    ) -> Self {
        let copilot = AppState::try_global(cx)
            .and_then(|state| GlobalCopilotAuth::try_get_or_init(state, cx));

        Self {
            copilot_status: copilot.as_ref().map(|copilot| copilot.0.read(cx).status()),
            is_authenticated: Box::new(is_authenticated),
            edit_prediction: matches!(mode, ConfigurationMode::EditPrediction),
            _subscription: copilot.as_ref().map(|copilot| {
                cx.observe(&copilot.0, |this, model, cx| {
                    this.copilot_status = Some(model.read(cx).status());
                    cx.notify();
                })
            }),
        }
    }
}
