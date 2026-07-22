use anyhow::Context as _;
use gpui::{
    App, AppContext as _, AsyncWindowContext, ClickEvent, Context, DismissEvent, Entity,
    EventEmitter, FocusHandle, Focusable, PromptLevel, Render, ScrollHandle, Task,
};
use markdown::{CopyButtonVisibility, Markdown, MarkdownElement};
use ui::{CopyButton, Tooltip, prelude::*};
use util::ResultExt;

use crate::{SuppressNotification, Workspace};

use super::{Notification, SuppressEvent, markdown_style::markdown_style};

pub struct LanguageServerPrompt {
    focus_handle: FocusHandle,
    pub(super) request: Option<project::LanguageServerPromptRequest>,
    scroll_handle: ScrollHandle,
    markdown: Entity<Markdown>,
    pub(super) dismiss_task: Option<Task<()>>,
}

impl Focusable for LanguageServerPrompt {
    fn focus_handle(&self, _cx: &App) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

impl Notification for LanguageServerPrompt {}

impl LanguageServerPrompt {
    pub fn new(request: project::LanguageServerPromptRequest, cx: &mut App) -> Self {
        let markdown = cx.new(|cx| Markdown::new(request.message.clone().into(), None, None, cx));

        Self {
            focus_handle: cx.focus_handle(),
            request: Some(request),
            scroll_handle: ScrollHandle::new(),
            markdown,
            dismiss_task: None,
        }
    }

    async fn select_option(this: Entity<Self>, ix: usize, cx: &mut AsyncWindowContext) {
        util::maybe!(async move {
            let potential_future = this.update(cx, |this, _| {
                this.request.take().map(|request| request.respond(ix))
            });

            potential_future
                .context("Response already sent")?
                .await
                .context("Stream already closed")?;

            this.update(cx, |this, cx| {
                this.dismiss_notification(cx);
            });

            anyhow::Ok(())
        })
        .await
        .log_err();
    }

    fn dismiss_notification(&mut self, cx: &mut Context<Self>) {
        self.dismiss_task = None;
        cx.emit(DismissEvent);
    }
}

impl Render for LanguageServerPrompt {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(request) = &self.request else {
            return div().id("language_server_prompt_notification");
        };

        let (icon, color) = match request.level {
            PromptLevel::Info => (IconName::Info, Color::Muted),
            PromptLevel::Warning => (IconName::Warning, Color::Warning),
            PromptLevel::Critical => (IconName::XCircle, Color::Error),
        };

        let suppress = window.modifiers().shift;
        let (close_id, close_icon) = if suppress {
            ("suppress", IconName::Minimize)
        } else {
            ("close", IconName::Close)
        };

        div()
            .id("language_server_prompt_notification")
            .group("language_server_prompt_notification")
            .occlude()
            .w_full()
            .max_h(vh(0.8, window))
            .elevation_3(cx)
            .overflow_y_scroll()
            .track_scroll(&self.scroll_handle)
            .on_modifiers_changed(cx.listener(|_, _, _, cx| cx.notify()))
            .child(
                v_flex()
                    .p_3()
                    .overflow_hidden()
                    .child(
                        h_flex()
                            .justify_between()
                            .child(
                                h_flex()
                                    .gap_2()
                                    .child(Icon::new(icon).color(color).size(IconSize::Small))
                                    .child(Label::new(request.lsp_name.clone())),
                            )
                            .child(
                                h_flex()
                                    .gap_1()
                                    .child(
                                        CopyButton::new(
                                            "copy-description",
                                            request.message.clone(),
                                        )
                                        .tooltip_label("Copy Description"),
                                    )
                                    .child(
                                        IconButton::new(close_id, close_icon)
                                            .tooltip(move |_window, cx| {
                                                if suppress {
                                                    Tooltip::with_meta(
                                                        "Suppress",
                                                        Some(&SuppressNotification),
                                                        "Click to close",
                                                        cx,
                                                    )
                                                } else {
                                                    Tooltip::with_meta(
                                                        "Close",
                                                        Some(&menu::Cancel),
                                                        "Suppress with shift-click",
                                                        cx,
                                                    )
                                                }
                                            })
                                            .on_click(cx.listener(
                                                move |this, _: &ClickEvent, _, cx| {
                                                    if suppress {
                                                        cx.emit(SuppressEvent);
                                                    } else {
                                                        this.dismiss_notification(cx);
                                                    }
                                                },
                                            )),
                                    ),
                            ),
                    )
                    .child(
                        MarkdownElement::new(self.markdown.clone(), markdown_style(window, cx))
                            .text_size(TextSize::Small.rems(cx))
                            .code_block_renderer(markdown::CodeBlockRenderer::Default {
                                copy_button_visibility: CopyButtonVisibility::Hidden,
                                wrap_button_visibility: markdown::WrapButtonVisibility::Hidden,
                                border: false,
                            })
                            .on_url_click(|link, window, cx| {
                                if let Some(workspace) = Workspace::for_window(window, cx) {
                                    workspace.update(cx, |workspace, cx| {
                                        workspace.open_url_or_file(&link, None, window, cx);
                                    });
                                } else {
                                    cx.open_url(&link);
                                }
                            }),
                    )
                    .children(request.actions.iter().enumerate().map(|(ix, action)| {
                        let this_handle = cx.entity();
                        Button::new(ix, action.title.clone())
                            .size(ButtonSize::Large)
                            .on_click(move |_, window, cx| {
                                let this_handle = this_handle.clone();
                                window
                                    .spawn(cx, async move |cx| {
                                        LanguageServerPrompt::select_option(this_handle, ix, cx)
                                            .await
                                    })
                                    .detach()
                            })
                    })),
            )
    }
}

impl EventEmitter<DismissEvent> for LanguageServerPrompt {}
impl EventEmitter<SuppressEvent> for LanguageServerPrompt {}
