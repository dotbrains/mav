use crate::{TestServer, test_server::open_channel_notes};
use call::ActiveCall;
use channel::ACKNOWLEDGE_DEBOUNCE_INTERVAL;
use client::{Collaborator, LegacyUserId, ParticipantIndex};
use collab::rpc::{CLEANUP_TIMEOUT, RECONNECT_TIMEOUT};

use collab_ui::channel_view::ChannelView;
use collections::HashMap;
use editor::{Anchor, Editor, MultiBufferOffset, ToOffset};
use futures::future;
use gpui::{BackgroundExecutor, Context, Entity, TestAppContext, Window};
use rpc::{RECEIVE_TIMEOUT, proto::PeerId};
use serde_json::json;
use std::ops::Range;
use util::rel_path::rel_path;
use workspace::CollaboratorId;

mod change_tracking;
mod core;
mod handles_and_reconnect;
mod lost_operations;
mod participant_indices;
mod test_support;
