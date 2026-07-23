use copilot::{GlobalCopilotAuth, Status};
use gpui::{Context, IntoElement, ParentElement, Render, Styled, Window};
use ui::{ConfiguredApiCard, prelude::*};
use workspace::AppState;

use super::{
    ConfigurationView, ERROR_LABEL, initiate_sign_in, initiate_sign_out, reinstall_and_sign_in,
};

impl ConfigurationView {
    fn is_starting(&self) -> bool {
        matches!(&self.copilot_status, Some(Status::Starting { .. }))
    }

    fn is_signing_in(&self) -> bool {
        matches!(
            &self.copilot_status,
            Some(Status::SigningIn { .. })
                | Some(Status::SignedOut {
                    awaiting_signing_in: true
                })
        )
    }

    fn is_error(&self) -> bool {
        matches!(&self.copilot_status, Some(Status::Error(_)))
    }

    fn has_no_status(&self) -> bool {
        self.copilot_status.is_none()
    }

    fn loading_message(&self) -> Option<SharedString> {
        if self.is_starting() {
            Some("Starting Copilot…".into())
        } else if self.is_signing_in() {
            Some("Signing into Copilot…".into())
        } else {
            None
        }
    }

    fn render_loading_button(
        &self,
        label: impl Into<SharedString>,
        edit_prediction: bool,
    ) -> impl IntoElement {
        Button::new("loading_button", label)
            .full_width()
            .disabled(true)
            .loading(true)
            .style(ButtonStyle::Outlined)
            .when(edit_prediction, |this| this.size(ButtonSize::Medium))
    }

    fn render_sign_in_button(&self, edit_prediction: bool) -> impl IntoElement {
        let label = if edit_prediction {
            "Sign in to GitHub"
        } else {
            "Sign in to use GitHub Copilot"
        };

        Button::new("sign_in", label)
            .map(|this| {
                if edit_prediction {
                    this.size(ButtonSize::Medium)
                } else {
                    this.full_width()
                }
            })
            .style(ButtonStyle::Outlined)
            .start_icon(
                Icon::new(IconName::Github)
                    .size(IconSize::Small)
                    .color(Color::Muted),
            )
            .when(edit_prediction, |this| this.tab_index(0isize))
            .on_click(|_, window, cx| {
                let app_state = AppState::global(cx);
                if let Some(copilot) = GlobalCopilotAuth::try_get_or_init(app_state, cx) {
                    initiate_sign_in(copilot.0, window, cx)
                }
            })
    }

    fn render_reinstall_button(&self, edit_prediction: bool) -> impl IntoElement {
        let label = if edit_prediction {
            "Reinstall and Sign in"
        } else {
            "Reinstall Copilot and Sign in"
        };

        Button::new("reinstall_and_sign_in", label)
            .map(|this| {
                if edit_prediction {
                    this.size(ButtonSize::Medium)
                } else {
                    this.full_width()
                }
            })
            .style(ButtonStyle::Outlined)
            .start_icon(
                Icon::new(IconName::Download)
                    .size(IconSize::Small)
                    .color(Color::Muted),
            )
            .on_click(|_, window, cx| {
                let app_state = AppState::global(cx);
                if let Some(copilot) = GlobalCopilotAuth::try_get_or_init(app_state, cx) {
                    reinstall_and_sign_in(copilot.0, window, cx);
                }
            })
    }

    fn render_for_edit_prediction(&self) -> impl IntoElement {
        let container = |description: SharedString, action: AnyElement| {
            h_flex()
                .pt_2p5()
                .w_full()
                .justify_between()
                .child(
                    v_flex()
                        .w_full()
                        .max_w_1_2()
                        .child(Label::new("Authenticate To Use"))
                        .child(
                            Label::new(description)
                                .color(Color::Muted)
                                .size(LabelSize::Small),
                        ),
                )
                .child(action)
        };

        let start_label = "To use Copilot for edit predictions, you need to be logged in to GitHub. Note that your GitHub account must have an active Copilot subscription.".into();
        let no_status_label = "Copilot requires an active GitHub Copilot subscription. Please ensure Copilot is configured and try again, or use a different edit predictions provider.".into();

        if let Some(msg) = self.loading_message() {
            container(
                start_label,
                self.render_loading_button(msg, true).into_any_element(),
            )
            .into_any_element()
        } else if self.is_error() {
            container(
                ERROR_LABEL.into(),
                self.render_reinstall_button(true).into_any_element(),
            )
            .into_any_element()
        } else if self.has_no_status() {
            container(
                no_status_label,
                self.render_sign_in_button(true).into_any_element(),
            )
            .into_any_element()
        } else {
            container(
                start_label,
                self.render_sign_in_button(true).into_any_element(),
            )
            .into_any_element()
        }
    }

    fn render_for_chat(&self) -> impl IntoElement {
        let start_label = "To use Mav's agent with GitHub Copilot, you need to be logged in to GitHub. Note that your GitHub account must have an active Copilot Chat subscription.";
        let no_status_label = "Copilot Chat requires an active GitHub Copilot subscription. Please ensure Copilot is configured and try again, or use a different LLM provider.";

        if let Some(msg) = self.loading_message() {
            v_flex()
                .gap_2()
                .child(Label::new(start_label))
                .child(self.render_loading_button(msg, false))
                .into_any_element()
        } else if self.is_error() {
            v_flex()
                .gap_2()
                .child(Label::new(ERROR_LABEL))
                .child(self.render_reinstall_button(false))
                .into_any_element()
        } else if self.has_no_status() {
            v_flex()
                .gap_2()
                .child(Label::new(no_status_label))
                .child(self.render_sign_in_button(false))
                .into_any_element()
        } else {
            v_flex()
                .gap_2()
                .child(Label::new(start_label))
                .child(self.render_sign_in_button(false))
                .into_any_element()
        }
    }
}

impl Render for ConfigurationView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let is_authenticated = &self.is_authenticated;

        if is_authenticated(cx) {
            return ConfiguredApiCard::new("Authorized")
                .button_label("Sign Out")
                .on_click(|_, window, cx| {
                    if let Some(auth) = GlobalCopilotAuth::try_global(cx) {
                        initiate_sign_out(auth.0.clone(), window, cx);
                    }
                })
                .into_any_element();
        }

        if self.edit_prediction {
            self.render_for_edit_prediction().into_any_element()
        } else {
            self.render_for_chat().into_any_element()
        }
    }
}
