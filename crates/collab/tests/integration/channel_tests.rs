use crate::{RoomParticipants, TestServer, room_participants};
use call::ActiveCall;
use channel::{ChannelMembership, ChannelStore};
use client::{ChannelId, User};
use collab::{
    db::{self, UserId},
    rpc::RECONNECT_TIMEOUT,
};
use futures::future::try_join_all;
use gpui::{BackgroundExecutor, Entity, SharedString, TestAppContext};
use rpc::{
    RECEIVE_TIMEOUT,
    proto::{self, ChannelRole},
};
use std::sync::Arc;

mod access;
mod ancestor_member;
mod call_from_channel;
mod channel_jumping;
mod channel_rename;
mod channel_room;
mod core_channels;
mod leave_and_move;
mod link_notifications;
mod lost_channel_creation;
mod membership_notifications;
mod permissions_update;
mod test_support;
