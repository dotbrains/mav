use gpui::{Context, Entity, Task, TaskExt, Window};
use ui::{ButtonLink, ConfiguredApiCard, List, ListBulletItem, prelude::*};
use ui_input::InputField;
use util::ResultExt;
use x_ai::XAI_API_URL;

use super::{API_KEY_ENV_VAR_NAME, State, XAiLanguageModelProvider};

pub(super) struct ConfigurationView {
    api_key_editor: Entity<InputField>,
    state: Entity<State>,
    load_credentials_task: Option<Task<()>>,
}

impl ConfigurationView {
    pub(super) fn new(state: Entity<State>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let api_key_editor = cx.new(|cx| {
            InputField::new(
                window,
                cx,
                "xai-0000000000000000000000000000000000000000000000000",
            )
            .label("API key")
        });

        cx.observe(&state, |_, _, cx| {
            cx.notify();
        })
        .detach();

        let load_credentials_task = Some(cx.spawn_in(window, {
            let state = state.clone();
            async move |this, cx| {
                if let Some(task) = Some(state.update(cx, |state, cx| state.authenticate(cx))) {
                    // We don't log an error, because "not signed in" is also an error.
                    let _ = task.await;
                }
                this.update(cx, |this, cx| {
                    this.load_credentials_task = None;
                    cx.notify();
                })
                .log_err();
            }
        }));

        Self {
            api_key_editor,
            state,
            load_credentials_task,
        }
    }

    fn save_api_key(&mut self, _: &menu::Confirm, window: &mut Window, cx: &mut Context<Self>) {
        let api_key = self.api_key_editor.read(cx).text(cx).trim().to_string();
        if api_key.is_empty() {
            return;
        }

        // url changes can cause the editor to be displayed again
        self.api_key_editor
            .update(cx, |editor, cx| editor.set_text("", window, cx));

        let state = self.state.clone();
        cx.spawn_in(window, async move |_, cx| {
            state
                .update(cx, |state, cx| state.set_api_key(Some(api_key), cx))
                .await
        })
        .detach_and_log_err(cx);
    }

    fn reset_api_key(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.api_key_editor
            .update(cx, |input, cx| input.set_text("", window, cx));

        let state = self.state.clone();
        cx.spawn_in(window, async move |_, cx| {
            state
                .update(cx, |state, cx| state.set_api_key(None, cx))
                .await
        })
        .detach_and_log_err(cx);
    }

    fn should_render_editor(&self, cx: &mut Context<Self>) -> bool {
        !self.state.read(cx).is_authenticated()
    }
}

impl Render for ConfigurationView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let env_var_set = self.state.read(cx).api_key_state.is_from_env_var();
        let configured_card_label = if env_var_set {
            format!("API key set in {API_KEY_ENV_VAR_NAME} environment variable")
        } else {
            let api_url = XAiLanguageModelProvider::api_url(cx);
            if api_url == XAI_API_URL {
                "API key configured".to_string()
            } else {
                format!("API key configured for {}", api_url)
            }
        };

        let api_key_section = if self.should_render_editor(cx) {
            v_flex()
                .on_action(cx.listener(Self::save_api_key))
                .child(Label::new(
                    "To use Mav's agent with xAI, you need to add an API key. Follow these steps:",
                ))
                .child(
                    List::new()
                        .child(
                            ListBulletItem::new("")
                                .child(Label::new("Create one by visiting"))
                                .child(ButtonLink::new(
                                    "xAI console",
                                    "https://console.x.ai/team/default/api-keys",
                                )),
                        )
                        .child(ListBulletItem::new(
                            "Paste your API key below and hit enter to start using the agent",
                        )),
                )
                .child(self.api_key_editor.clone())
                .child(
                    Label::new(format!(
                        "You can also set the {API_KEY_ENV_VAR_NAME} environment variable and restart Mav."
                    ))
                    .size(LabelSize::Small)
                    .color(Color::Muted),
                )
                .child(
                    Label::new("Note that xAI is a custom OpenAI-compatible provider.")
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                )
                .into_any_element()
        } else {
            ConfiguredApiCard::new(configured_card_label)
                .disabled(env_var_set)
                .when(env_var_set, |this| {
                    this.tooltip_label(format!(
                        "To reset your API key, unset the {API_KEY_ENV_VAR_NAME} environment variable."
                    ))
                })
                .on_click(cx.listener(|this, _, window, cx| this.reset_api_key(window, cx)))
                .into_any_element()
        };

        if self.load_credentials_task.is_some() {
            div().child(Label::new("Loading credentials...")).into_any()
        } else {
            v_flex().size_full().child(api_key_section).into_any()
        }
    }
}
