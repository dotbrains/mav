use crate::{AudioStream, Participant, RemoteTrack, RoomEvent, TrackPublication};

use crate::mock_client::{participant::*, publication::*, track::*};
use anyhow::{Context as _, Result};
use async_trait::async_trait;
use collections::{BTreeMap, HashMap, HashSet, btree_map::Entry as BTreeEntry, hash_map::Entry};
use gpui::{App, AsyncApp, BackgroundExecutor};
use livekit_api::{proto, token};
use parking_lot::Mutex;
use postage::{mpsc, sink::Sink};
use std::sync::{
    Arc, Weak,
    atomic::{AtomicBool, AtomicU64, Ordering::SeqCst},
};

mod api_client;
mod room;
mod server;
mod server_tracks;
mod server_types;
mod types;

pub use api_client::TestApiClient;
pub(crate) use room::WeakRoom;
pub use room::{Room, RoomState};
pub use server::TestServer;
pub(crate) use server_types::{TestServerAudioTrack, TestServerVideoTrack};
pub use types::{ConnectionState, ParticipantIdentity, RtcStats, SessionStats, TrackSid};
