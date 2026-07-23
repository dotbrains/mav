use copilot::{
    Copilot,
    request::{self, PromptUserDeviceFlow},
};
use gpui::{
    ClipboardItem, Context, DismissEvent, Element, Entity, IntoElement, ParentElement, Styled,
};
use project::project_settings::ProjectSettings;
use settings::Settings as _;
use ui::{ButtonLike, prelude::*};

use super::{COPILOT_SIGN_UP_URL, CopilotCodeVerification, ERROR_LABEL, reinstall_and_sign_in};

fn render_device_code(
    data: &PromptUserDeviceFlow,
    cx: &mut Context<CopilotCodeVerification>,
) -> impl IntoElement {
    let copied = cx
        .read_from_clipboard()
        .map(|item| item.text().as_ref() == Some(&data.user_code))
        .unwrap_or(false);

    ButtonLike::new("copy-button")
        .full_width()
        .style(ButtonStyle::Tinted(ui::TintColor::Accent))
        .size(ButtonSize::Medium)
        .child(
            h_flex()
                .w_full()
                .p_1()
                .justify_between()
                .child(Label::new(data.user_code.clone()))
                .child(Label::new(if copied { "Copied!" } else { "Copy" })),
        )
        .on_click({
            let user_code = data.user_code.clone();
            move |_, window, cx| {
                cx.write_to_clipboard(ClipboardItem::new_string(user_code.clone()));
                window.refresh();
            }
        })
}

pub(super) fn render_prompting_modal(
    copilot: Entity<Copilot>,
    connect_clicked: bool,
    data: &PromptUserDeviceFlow,
    cx: &mut Context<CopilotCodeVerification>,
) -> impl Element {
    let connect_button_label = if connect_clicked {
        "Waiting for connection…"
    } else {
        "Connect to GitHub"
    };

    v_flex()
        .flex_1()
        .gap_2p5()
        .items_center()
        .text_center()
        .child(Headline::new("Use GitHub Copilot in Mav").size(HeadlineSize::Large))
        .child(
            Label::new("Using Copilot requires an active subscription on GitHub.")
                .color(Color::Muted),
        )
        .child(render_device_code(data, cx))
        .child(
            Label::new("Paste this code into GitHub after clicking the button below.")
                .color(Color::Muted),
        )
        .child(
            v_flex()
                .w_full()
                .gap_1()
                .child(
                    Button::new("connect-button", connect_button_label)
                        .full_width()
                        .style(ButtonStyle::Outlined)
                        .size(ButtonSize::Medium)
                        .on_click({
                            let command = data.command.clone();
                            cx.listener(move |this, _, _window, cx| {
                                let command = command.clone();
                                let copilot_clone = copilot.clone();
                                let request_timeout = ProjectSettings::get_global(cx)
                                    .global_lsp_settings
                                    .get_request_timeout();
                                copilot.update(cx, |copilot, cx| {
                                    if let Some(server) = copilot.language_server() {
                                        let server = server.clone();
                                        cx.spawn(async move |_, cx| {
                                            let result = server
                                                .request::<lsp::request::ExecuteCommand>(
                                                    lsp::ExecuteCommandParams {
                                                        command: command.command.clone(),
                                                        arguments: command
                                                            .arguments
                                                            .clone()
                                                            .unwrap_or_default(),
                                                        ..Default::default()
                                                    },
                                                    request_timeout,
                                                )
                                                .await
                                                .into_response()
                                                .ok()
                                                .flatten();
                                            if let Some(value) = result {
                                                if let Ok(status) =
                                                    serde_json::from_value::<request::SignInStatus>(
                                                        value,
                                                    )
                                                {
                                                    copilot_clone.update(cx, |copilot, cx| {
                                                        copilot.update_sign_in_status(status, cx);
                                                    });
                                                }
                                            }
                                        })
                                        .detach();
                                    }
                                });

                                this.connect_clicked = true;
                            })
                        }),
                )
                .child(
                    Button::new("copilot-enable-cancel-button", "Cancel")
                        .full_width()
                        .size(ButtonSize::Medium)
                        .on_click(cx.listener(|_, _, _, cx| {
                            cx.emit(DismissEvent);
                        })),
                ),
        )
}

pub(super) fn render_enabled_modal(cx: &mut Context<CopilotCodeVerification>) -> impl Element {
    v_flex()
        .gap_2()
        .text_center()
        .justify_center()
        .child(Headline::new("Copilot Enabled!").size(HeadlineSize::Large))
        .child(Label::new("You're all set to use GitHub Copilot.").color(Color::Muted))
        .child(
            Button::new("copilot-enabled-done-button", "Done")
                .full_width()
                .style(ButtonStyle::Outlined)
                .size(ButtonSize::Medium)
                .on_click(cx.listener(|_, _, _, cx| cx.emit(DismissEvent))),
        )
}

pub(super) fn render_unauthorized_modal(
    sign_up_url: Option<&str>,
    cx: &mut Context<CopilotCodeVerification>,
) -> impl Element {
    let sign_up_url = sign_up_url.unwrap_or(COPILOT_SIGN_UP_URL).to_owned();
    let description = "Enable Copilot by connecting your existing license once you have subscribed or renewed your subscription.";

    v_flex()
        .gap_2()
        .text_center()
        .justify_center()
        .child(
            Headline::new("You must have an active GitHub Copilot subscription.")
                .size(HeadlineSize::Large),
        )
        .child(Label::new(description).color(Color::Warning))
        .child(
            Button::new("copilot-subscribe-button", "Subscribe on GitHub")
                .full_width()
                .style(ButtonStyle::Outlined)
                .size(ButtonSize::Medium)
                .on_click(move |_, _, cx| cx.open_url(&sign_up_url)),
        )
        .child(
            Button::new("copilot-subscribe-cancel-button", "Cancel")
                .full_width()
                .size(ButtonSize::Medium)
                .on_click(cx.listener(|_, _, _, cx| cx.emit(DismissEvent))),
        )
}

pub(super) fn render_error_modal(
    copilot: Entity<Copilot>,
    _cx: &mut Context<CopilotCodeVerification>,
) -> impl Element {
    v_flex()
        .gap_2()
        .text_center()
        .justify_center()
        .child(Headline::new("An Error Happened").size(HeadlineSize::Large))
        .child(Label::new(ERROR_LABEL).color(Color::Muted))
        .child(
            Button::new("copilot-subscribe-button", "Reinstall Copilot and Sign In")
                .full_width()
                .style(ButtonStyle::Outlined)
                .size(ButtonSize::Medium)
                .start_icon(
                    Icon::new(IconName::Download)
                        .size(IconSize::Small)
                        .color(Color::Muted),
                )
                .on_click(move |_, window, cx| reinstall_and_sign_in(copilot.clone(), window, cx)),
        )
}
