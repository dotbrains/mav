use channel::{ChannelMembership, ChannelStore};
use client::{
    ChannelId, LegacyUserId, User, UserStore,
    proto::{self, ChannelRole, ChannelVisibility},
};
use fuzzy::{StringMatchCandidate, match_strings};
use gpui::{
    App, ClipboardItem, Context, DismissEvent, Entity, EventEmitter, Focusable, ParentElement,
    Render, Styled, Subscription, Task, TaskExt, WeakEntity, Window, actions, anchored, deferred,
    div,
};
use picker::{Picker, PickerDelegate};
use std::sync::Arc;
use ui::{Avatar, Checkbox, ContextMenu, ListItem, ListItemSpacing, prelude::*};
use util::TryFutureExt;
use workspace::{ModalView, notifications::DetachAndPromptErr};

actions!(
    channel_modal,
    [
        /// Selects the next control in the channel modal.
        SelectNextControl,
        /// Toggles between invite members and manage members mode.
        ToggleMode,
        /// Toggles admin status for the selected member.
        ToggleMemberAdmin,
        /// Removes the selected member from the channel.
        RemoveMember
    ]
);

pub struct ChannelModal {
    picker: Entity<Picker<ChannelModalDelegate>>,
    channel_store: Entity<ChannelStore>,
    channel_id: ChannelId,
}

impl ChannelModal {
    pub fn new(
        user_store: Entity<UserStore>,
        channel_store: Entity<ChannelStore>,
        channel_id: ChannelId,
        mode: Mode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&channel_store, |_, _, cx| cx.notify()).detach();
        let channel_modal = cx.entity().downgrade();
        let picker = cx.new(|cx| {
            Picker::uniform_list(
                ChannelModalDelegate {
                    channel_modal,
                    matching_users: Vec::new(),
                    matching_member_indices: Vec::new(),
                    selected_index: 0,
                    user_store: user_store.clone(),
                    channel_store: channel_store.clone(),
                    channel_id,
                    match_candidates: Vec::new(),
                    context_menu: None,
                    members: Vec::new(),
                    has_all_members: false,
                    mode,
                },
                window,
                cx,
            )
            .embedded()
        });

        Self {
            picker,
            channel_store,
            channel_id,
        }
    }

    fn toggle_mode(&mut self, _: &ToggleMode, window: &mut Window, cx: &mut Context<Self>) {
        let mode = match self.picker.read(cx).delegate.mode {
            Mode::ManageMembers => Mode::InviteMembers,
            Mode::InviteMembers => Mode::ManageMembers,
        };
        self.set_mode(mode, window, cx);
    }

    fn set_mode(&mut self, mode: Mode, window: &mut Window, cx: &mut Context<Self>) {
        self.picker.update(cx, |picker, cx| {
            let delegate = &mut picker.delegate;
            delegate.mode = mode;
            delegate.selected_index = 0;
            picker.set_query("", window, cx);
            picker.update_matches(picker.query(cx), window, cx);
            cx.notify()
        });
        cx.notify()
    }

    fn set_channel_visibility(
        &mut self,
        selection: &ToggleState,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.channel_store.update(cx, |channel_store, cx| {
            channel_store
                .set_channel_visibility(
                    self.channel_id,
                    match selection {
                        ToggleState::Unselected => ChannelVisibility::Members,
                        ToggleState::Selected => ChannelVisibility::Public,
                        ToggleState::Indeterminate => return,
                    },
                    cx,
                )
                .detach_and_log_err(cx)
        });
    }

    fn dismiss(&mut self, _: &menu::Cancel, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(DismissEvent);
    }
}

impl EventEmitter<DismissEvent> for ChannelModal {}
impl ModalView for ChannelModal {}

impl Focusable for ChannelModal {
    fn focus_handle(&self, cx: &App) -> gpui::FocusHandle {
        self.picker.focus_handle(cx)
    }
}

impl Render for ChannelModal {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let channel_store = self.channel_store.read(cx);
        let Some(channel) = channel_store.channel_for_id(self.channel_id) else {
            return div();
        };
        let channel_name = channel.name.clone();
        let channel_id = channel.id;
        let visibility = channel.visibility;
        let mode = self.picker.read(cx).delegate.mode;

        v_flex()
            .key_context("ChannelModal")
            .on_action(cx.listener(Self::toggle_mode))
            .on_action(cx.listener(Self::dismiss))
            .elevation_3(cx)
            .child(
                v_flex()
                    .px_2()
                    .py_1()
                    .gap_2()
                    .child(
                        h_flex()
                            .w_px()
                            .flex_1()
                            .gap_1()
                            .child(Icon::new(IconName::Hash).size(IconSize::Medium))
                            .child(Label::new(channel_name)),
                    )
                    .child(
                        h_flex()
                            .w_full()
                            .h(rems_from_px(22.))
                            .justify_between()
                            .line_height(rems(1.25))
                            .child(
                                Checkbox::new(
                                    "is-public",
                                    if visibility == ChannelVisibility::Public {
                                        ui::ToggleState::Selected
                                    } else {
                                        ui::ToggleState::Unselected
                                    },
                                )
                                .label("Public")
                                .on_click(cx.listener(Self::set_channel_visibility)),
                            )
                            .children(
                                Some(
                                    Button::new("copy-link", "Copy Link")
                                        .label_size(LabelSize::Small)
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            if let Some(channel) = this
                                                .channel_store
                                                .read(cx)
                                                .channel_for_id(channel_id)
                                            {
                                                let item =
                                                    ClipboardItem::new_string(channel.link(cx));
                                                cx.write_to_clipboard(item);
                                            }
                                        })),
                                )
                                .filter(|_| visibility == ChannelVisibility::Public),
                            ),
                    )
                    .child(
                        h_flex()
                            .child(
                                div()
                                    .id("manage-members")
                                    .px_2()
                                    .py_1()
                                    .cursor_pointer()
                                    .border_b_2()
                                    .when(mode == Mode::ManageMembers, |this| {
                                        this.border_color(cx.theme().colors().border)
                                    })
                                    .child(Label::new("Manage Members"))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.set_mode(Mode::ManageMembers, window, cx);
                                    })),
                            )
                            .child(
                                div()
                                    .id("invite-members")
                                    .px_2()
                                    .py_1()
                                    .cursor_pointer()
                                    .border_b_2()
                                    .when(mode == Mode::InviteMembers, |this| {
                                        this.border_color(cx.theme().colors().border)
                                    })
                                    .child(Label::new("Invite Members"))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.set_mode(Mode::InviteMembers, window, cx);
                                    })),
                            ),
                    ),
            )
            .child(self.picker.clone())
    }
}

#[derive(Copy, Clone, PartialEq)]
pub enum Mode {
    ManageMembers,
    InviteMembers,
}

pub struct ChannelModalDelegate {
    channel_modal: WeakEntity<ChannelModal>,
    matching_users: Vec<Arc<User>>,
    matching_member_indices: Vec<usize>,
    user_store: Entity<UserStore>,
    channel_store: Entity<ChannelStore>,
    channel_id: ChannelId,
    selected_index: usize,
    mode: Mode,
    match_candidates: Vec<StringMatchCandidate>,
    members: Vec<ChannelMembership>,
    has_all_members: bool,
    context_menu: Option<(Entity<ContextMenu>, Subscription)>,
}

#[path = "channel_modal/delegate_methods.rs"]
mod delegate_methods;
#[path = "channel_modal/picker_delegate.rs"]
mod picker_delegate;
