use std::sync::Arc;

use askpass::EncryptedPassword;
use futures::channel::oneshot;
use gpui::{
    Entity, FontFeatures, ParentElement as _, Render, SharedString, TextStyleRefinement, Window,
};
use markdown::{Markdown, MarkdownElement, MarkdownStyle};
use settings::Settings;
use theme_settings::ThemeSettings;
use ui::{ActiveTheme, CommonAnimationExt, Context, InteractiveElement, Tooltip, prelude::*};
use ui_input::{ERASED_EDITOR_FACTORY, ErasedEditor};

pub struct RemoteConnectionPrompt {
    pub(super) connection_string: SharedString,
    pub(super) nickname: Option<SharedString>,
    pub(super) is_wsl: bool,
    pub(super) is_devcontainer: bool,
    status_message: Option<SharedString>,
    prompt: Option<(Entity<Markdown>, oneshot::Sender<EncryptedPassword>)>,
    pub(super) cancellation: Option<oneshot::Sender<()>>,
    pub(super) editor: Arc<dyn ErasedEditor>,
    is_password_prompt: bool,
    is_masked: bool,
}

impl Drop for RemoteConnectionPrompt {
    fn drop(&mut self) {
        if let Some(cancel) = self.cancellation.take() {
            log::debug!("cancelling remote connection");
            cancel.send(()).ok();
        }
    }
}

impl RemoteConnectionPrompt {
    pub fn new(
        connection_string: String,
        nickname: Option<String>,
        is_wsl: bool,
        is_devcontainer: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let editor_factory = ERASED_EDITOR_FACTORY
            .get()
            .expect("ErasedEditorFactory to be initialized");
        let editor = (editor_factory)(window, cx);

        Self {
            connection_string: connection_string.into(),
            nickname: nickname.map(|nickname| nickname.into()),
            is_wsl,
            is_devcontainer,
            editor,
            status_message: None,
            cancellation: None,
            prompt: None,
            is_password_prompt: false,
            is_masked: true,
        }
    }

    pub fn set_cancellation_tx(&mut self, tx: oneshot::Sender<()>) {
        self.cancellation = Some(tx);
    }

    pub fn set_prompt(
        &mut self,
        prompt: String,
        tx: oneshot::Sender<EncryptedPassword>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let is_yes_no = prompt.contains("yes/no");
        self.is_password_prompt = !is_yes_no;
        self.is_masked = !is_yes_no;
        self.editor.set_masked(self.is_masked, window, cx);

        let markdown = cx.new(|cx| Markdown::new_text(prompt.into(), cx));
        self.prompt = Some((markdown, tx));
        self.status_message.take();
        window.focus(&self.editor.focus_handle(cx), cx);
        cx.notify();
    }

    pub fn set_status(&mut self, status: Option<String>, cx: &mut Context<Self>) {
        self.status_message = status.map(|s| s.into());
        cx.notify();
    }

    pub fn confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some((_, tx)) = self.prompt.take() {
            self.status_message = Some("Connecting".into());

            let pw = self.editor.text(cx);
            if let Ok(secure) = EncryptedPassword::try_from(pw.as_ref()) {
                tx.send(secure).ok();
            }
            self.editor.clear(window, cx);
        }
    }
}

impl Render for RemoteConnectionPrompt {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = ThemeSettings::get_global(cx);

        let mut text_style = window.text_style();
        let refinement = TextStyleRefinement {
            font_family: Some(theme.buffer_font.family.clone()),
            font_features: Some(FontFeatures::disable_ligatures()),
            font_size: Some(theme.buffer_font_size(cx).into()),
            color: Some(cx.theme().colors().editor_foreground),
            background_color: Some(gpui::transparent_black()),
            ..Default::default()
        };

        text_style.refine(&refinement);
        let markdown_style = MarkdownStyle {
            base_text_style: text_style,
            selection_background_color: cx.theme().colors().element_selection_background,
            ..Default::default()
        };

        let is_password_prompt = self.is_password_prompt;
        let is_masked = self.is_masked;
        let (masked_password_icon, masked_password_tooltip) = if is_masked {
            (IconName::Eye, "Toggle to Unmask Password")
        } else {
            (IconName::EyeOff, "Toggle to Mask Password")
        };

        v_flex()
            .key_context("PasswordPrompt")
            .p_2()
            .size_full()
            .when_some(self.prompt.as_ref(), |this, prompt| {
                this.child(
                    v_flex()
                        .text_sm()
                        .size_full()
                        .overflow_hidden()
                        .child(
                            h_flex()
                                .w_full()
                                .justify_between()
                                .child(MarkdownElement::new(prompt.0.clone(), markdown_style))
                                .when(is_password_prompt, |this| {
                                    this.child(
                                        IconButton::new("toggle_mask", masked_password_icon)
                                            .icon_size(IconSize::Small)
                                            .tooltip(Tooltip::text(masked_password_tooltip))
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.is_masked = !this.is_masked;
                                                this.editor.set_masked(this.is_masked, window, cx);
                                                window.focus(&this.editor.focus_handle(cx), cx);
                                                cx.notify();
                                            })),
                                    )
                                }),
                        )
                        .child(div().flex_1().child(self.editor.render(window, cx))),
                )
                .when(window.capslock().on, |this| {
                    this.child(
                        h_flex()
                            .py_0p5()
                            .min_w_0()
                            .w_full()
                            .gap_1()
                            .child(
                                Icon::new(IconName::Warning)
                                    .size(IconSize::Small)
                                    .color(Color::Muted),
                            )
                            .child(
                                Label::new("Caps lock is on.")
                                    .size(LabelSize::Small)
                                    .color(Color::Muted),
                            ),
                    )
                })
            })
            .when_some(self.status_message.clone(), |this, status_message| {
                this.child(
                    h_flex()
                        .min_w_0()
                        .w_full()
                        .mt_1()
                        .gap_1()
                        .child(
                            Icon::new(IconName::LoadCircle)
                                .size(IconSize::Small)
                                .color(Color::Muted)
                                .with_rotate_animation(2),
                        )
                        .child(
                            Label::new(format!("{}…", status_message))
                                .size(LabelSize::Small)
                                .color(Color::Muted)
                                .truncate()
                                .flex_1(),
                        ),
                )
            })
    }
}
