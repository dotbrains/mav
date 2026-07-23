use crate::multibuffer_hint::MultibufferHint;
use client::{Client, UserStore, mav_urls};
use cloud_api_types::Plan;
use gpui::{
    Action, AnyElement, App, AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable,
    IntoElement, KeyContext, Render, ScrollHandle, SharedString, Subscription, Task, WeakEntity,
    Window, actions,
};
use project::agent_server_store::AllAgentServersSettings;
use schemars::JsonSchema;
use serde::Deserialize;
use settings::SettingsStore;
use ui::{
    Divider, KeyBinding, ParentElement as _, StatefulInteractiveElement, Vector, VectorName,
    WithScrollbar as _, prelude::*, rems_from_px,
};

pub use workspace::welcome::ShowWelcome;
use workspace::welcome::WelcomePage;
use workspace::{
    Workspace, WorkspaceId,
    item::{Item, ItemEvent},
    notifications::NotifyResultExt as _,
    with_active_or_new_workspace,
};

mod actions;
mod base_keymap_picker;
mod basics_page;
mod import_settings;
pub mod multibuffer_hint;
mod persistence;
mod theme_preview;

pub use actions::{init, show_onboarding_view};
pub use import_settings::{SettingsImportState, handle_import_vscode_settings};

/// Imports settings from Visual Studio Code.
#[derive(Copy, Clone, Debug, Default, PartialEq, Deserialize, JsonSchema, Action)]
#[action(namespace = mav)]
#[serde(deny_unknown_fields)]
pub struct ImportVsCodeSettings {
    #[serde(default)]
    pub skip_prompt: bool,
}

/// Imports settings from Cursor editor.
#[derive(Copy, Clone, Debug, Default, PartialEq, Deserialize, JsonSchema, Action)]
#[action(namespace = mav)]
#[serde(deny_unknown_fields)]
pub struct ImportCursorSettings {
    #[serde(default)]
    pub skip_prompt: bool,
}

pub const FIRST_OPEN: &str = "first_open";

actions!(
    onboarding,
    [
        /// Finish the onboarding process.
        Finish,
        /// Sign in while in the onboarding flow.
        SignIn,
        /// Open the user account in mav.dev while in the onboarding flow.
        OpenAccount,
        /// Resets the welcome screen hints to their initial state.
        ResetHints
    ]
);

struct Onboarding {
    workspace: WeakEntity<Workspace>,
    focus_handle: FocusHandle,
    user_store: Entity<UserStore>,
    scroll_handle: ScrollHandle,
    _settings_subscription: Subscription,
}

impl Onboarding {
    fn new(workspace: &Workspace, cx: &mut App) -> Entity<Self> {
        let font_family_cache = theme::FontFamilyCache::global(cx);

        let installed_agents = cx
            .global::<SettingsStore>()
            .get::<AllAgentServersSettings>(None)
            .clone();
        let client = Client::global(cx);
        let status = *client.status().borrow();
        let plan = workspace.user_store().read(cx).plan();
        let mav_agent_state = if status.is_signed_out()
            || matches!(
                status,
                client::Status::AuthenticationError | client::Status::ConnectionError
            ) {
            "signed_out"
        } else if status.is_signing_in() {
            "signing_in"
        } else {
            match plan {
                Some(Plan::MavPro) => "pro",
                Some(Plan::MavProTrial) => "trial",
                Some(Plan::MavBusiness) => "business",
                Some(Plan::MavVip) => "vip",
                Some(Plan::MavStudent) => "student",
                Some(Plan::MavFree) | None => "free",
            }
        };
        let agents_installed = basics_page::FEATURED_AGENT_IDS
            .iter()
            .filter(|id| installed_agents.contains_key(**id))
            .copied()
            .collect::<Vec<_>>();
        telemetry::event!(
            "Welcome Agent Setup Viewed",
            mav_agent = mav_agent_state,
            agents_installed = agents_installed,
        );

        cx.new(|cx| {
            cx.spawn(async move |this, cx| {
                font_family_cache.prefetch(cx).await;
                this.update(cx, |_, cx| {
                    cx.notify();
                })
            })
            .detach();

            Self {
                workspace: workspace.weak_handle(),
                focus_handle: cx.focus_handle(),
                scroll_handle: ScrollHandle::new(),
                user_store: workspace.user_store().clone(),
                _settings_subscription: cx
                    .observe_global::<SettingsStore>(move |_, cx| cx.notify()),
            }
        })
    }

    fn on_finish(_: &Finish, _: &mut Window, cx: &mut App) {
        telemetry::event!("Finish Setup");
        go_to_welcome_page(cx);
    }

    fn handle_sign_in(&mut self, _: &SignIn, window: &mut Window, cx: &mut Context<Self>) {
        let client = Client::global(cx);
        let workspace = self.workspace.clone();

        window
            .spawn(cx, async move |mut cx| {
                client
                    .sign_in_with_optional_connect(true, &cx)
                    .await
                    .notify_workspace_async_err(workspace, &mut cx);
            })
            .detach();
    }

    fn handle_open_account(_: &OpenAccount, _: &mut Window, cx: &mut App) {
        cx.open_url(&mav_urls::account_url(cx))
    }

    fn render_page(&mut self, cx: &mut Context<Self>) -> AnyElement {
        crate::basics_page::render_basics_page(&self.user_store, cx).into_any_element()
    }
}

impl Render for Onboarding {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .image_cache(gpui::retain_all("onboarding-page"))
            .key_context({
                let mut ctx = KeyContext::new_with_defaults();
                ctx.add("Onboarding");
                ctx.add("menu");
                ctx
            })
            .track_focus(&self.focus_handle)
            .size_full()
            .bg(cx.theme().colors().editor_background)
            .on_action(Self::on_finish)
            .on_action(cx.listener(Self::handle_sign_in))
            .on_action(Self::handle_open_account)
            .on_action(cx.listener(|_, _: &menu::SelectNext, window, cx| {
                window.focus_next(cx);
                cx.notify();
            }))
            .on_action(cx.listener(|_, _: &menu::SelectPrevious, window, cx| {
                window.focus_prev(cx);
                cx.notify();
            }))
            .vertical_scrollbar_for(&self.scroll_handle, window, cx)
            .child(
                div()
                    .id("page-content")
                    .size_full()
                    .overflow_y_scroll()
                    .child(
                        v_flex()
                            .min_w_0()
                            .max_w(rems_from_px(780.))
                            .w_full()
                            .mx_auto()
                            .p_12()
                            .gap_6()
                            .child(
                                h_flex()
                                    .w_full()
                                    .gap_4()
                                    .justify_between()
                                    .child(
                                        h_flex()
                                            .gap_4()
                                            .child(Vector::square(VectorName::MavLogo, rems(2.5)))
                                            .child(
                                                v_flex()
                                                    .child(
                                                        Headline::new("Welcome to Mav")
                                                            .size(HeadlineSize::Small),
                                                    )
                                                    .child(
                                                        Label::new("The editor for what's next")
                                                            .color(Color::Muted)
                                                            .size(LabelSize::Small)
                                                            .italic(),
                                                    ),
                                            ),
                                    )
                                    .child({
                                        Button::new("finish_setup", "Finish Setup")
                                            .style(ButtonStyle::Filled)
                                            .size(ButtonSize::Medium)
                                            .width(rems_from_px(200.))
                                            .key_binding(KeyBinding::for_action_in(
                                                &Finish,
                                                &self.focus_handle,
                                                cx,
                                            ))
                                            .on_click(|_, window, cx| {
                                                window.dispatch_action(Finish.boxed_clone(), cx);
                                            })
                                    }),
                            )
                            .child(Divider::horizontal().color(ui::DividerColor::BorderVariant))
                            .child(self.render_page(cx)),
                    )
                    .track_scroll(&self.scroll_handle),
            )
    }
}

impl EventEmitter<ItemEvent> for Onboarding {}

impl Focusable for Onboarding {
    fn focus_handle(&self, _: &App) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

impl Item for Onboarding {
    type Event = ItemEvent;

    fn tab_content_text(&self, _detail: usize, _cx: &App) -> SharedString {
        "Onboarding".into()
    }

    fn telemetry_event_text(&self) -> Option<&'static str> {
        Some("Onboarding Page Opened")
    }

    fn show_toolbar(&self) -> bool {
        false
    }

    fn can_split(&self) -> bool {
        true
    }

    fn clone_on_split(
        &self,
        _workspace_id: Option<WorkspaceId>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Option<Entity<Self>>> {
        Task::ready(Some(cx.new(|cx| Onboarding {
            workspace: self.workspace.clone(),
            user_store: self.user_store.clone(),
            scroll_handle: ScrollHandle::new(),
            focus_handle: cx.focus_handle(),
            _settings_subscription: cx.observe_global::<SettingsStore>(move |_, cx| cx.notify()),
        })))
    }

    fn to_item_events(event: &Self::Event, f: &mut dyn FnMut(workspace::item::ItemEvent)) {
        f(*event)
    }
}

fn go_to_welcome_page(cx: &mut App) {
    with_active_or_new_workspace(cx, |workspace, window, cx| {
        let Some((onboarding_id, onboarding_idx)) = workspace
            .active_pane()
            .read(cx)
            .items()
            .enumerate()
            .find_map(|(idx, item)| {
                let _ = item.downcast::<Onboarding>()?;
                Some((item.item_id(), idx))
            })
        else {
            return;
        };

        workspace.active_pane().update(cx, |pane, cx| {
            // Get the index here to get around the borrow checker
            let idx = pane.items().enumerate().find_map(|(idx, item)| {
                let _ = item.downcast::<WelcomePage>()?;
                Some(idx)
            });

            if let Some(idx) = idx {
                pane.activate_item(idx, true, true, window, cx);
            } else {
                let item = Box::new(
                    cx.new(|cx| WelcomePage::new(workspace.weak_handle(), false, window, cx)),
                );
                pane.add_item(item, true, true, Some(onboarding_idx), window, cx);
            }

            pane.remove_item(onboarding_id, false, false, window, cx);
        });
    });
}

impl workspace::SerializableItem for Onboarding {
    fn serialized_item_kind() -> &'static str {
        "OnboardingPage"
    }

    fn cleanup(
        workspace_id: workspace::WorkspaceId,
        alive_items: Vec<workspace::ItemId>,
        _window: &mut Window,
        cx: &mut App,
    ) -> gpui::Task<gpui::Result<()>> {
        workspace::delete_unloaded_items(
            alive_items,
            workspace_id,
            "onboarding_pages",
            &persistence::OnboardingPagesDb::global(cx),
            cx,
        )
    }

    fn deserialize(
        _project: Entity<project::Project>,
        workspace: WeakEntity<Workspace>,
        workspace_id: workspace::WorkspaceId,
        item_id: workspace::ItemId,
        window: &mut Window,
        cx: &mut App,
    ) -> gpui::Task<gpui::Result<Entity<Self>>> {
        let db = persistence::OnboardingPagesDb::global(cx);
        window.spawn(cx, async move |cx| {
            if let Some(_) = db.get_onboarding_page(item_id, workspace_id)? {
                workspace.update(cx, |workspace, cx| Onboarding::new(workspace, cx))
            } else {
                Err(anyhow::anyhow!("No onboarding page to deserialize"))
            }
        })
    }

    fn serialize(
        &mut self,
        workspace: &mut Workspace,
        item_id: workspace::ItemId,
        _closing: bool,
        _window: &mut Window,
        cx: &mut ui::Context<Self>,
    ) -> Option<gpui::Task<gpui::Result<()>>> {
        let workspace_id = workspace.database_id()?;

        let db = persistence::OnboardingPagesDb::global(cx);
        Some(
            cx.background_spawn(
                async move { db.save_onboarding_page(item_id, workspace_id).await },
            ),
        )
    }

    fn should_serialize(&self, event: &Self::Event) -> bool {
        event == &ItemEvent::UpdateTab
    }
}
