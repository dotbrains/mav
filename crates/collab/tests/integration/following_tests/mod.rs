#![allow(clippy::reversed_empty_ranges)]
use crate::TestServer;
use call::ActiveCall;
use client::ChannelId;
use collab_ui::{
    channel_view::ChannelView,
    notifications::project_shared_notification::ProjectSharedNotification,
};
use editor::{Editor, MultiBuffer, MultiBufferOffset, PathKey, SelectionEffects};
use gpui::{
    Action, AppContext as _, BackgroundExecutor, BorrowAppContext, Entity, SharedString,
    TestAppContext, VisualContext, VisualTestContext, point,
};
use language::Capability;
use project::Project;
use rpc::proto::PeerId;
use serde_json::json;
use settings::SettingsStore;
use text::{Point, ToPoint};
use util::{path, rel_path::rel_path, test::sample_text};
use workspace::{
    CloseWindow, CollaboratorId, Item, MultiWorkspace, Pane, ParticipantLocation, SplitDirection,
    Workspace, item::ItemHandle as _,
};

use super::TestClient;

mod basic;
mod channel_notes;
mod excluded_file;
mod helpers;
mod mutual;
mod replacement;
mod tab_order;
mod unfollowing;

pub(crate) use helpers::join_channel;
use helpers::{assert_followed_tab_rotation, exercise_screen_share_following};
