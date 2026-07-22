mod call_handlers;
mod channel_admin;
mod channel_buffers;
mod channel_messages;
mod channel_subscriptions;
mod connection_pool;
#[path = "rpc/headers.rs"]
mod headers;
mod project_sharing;
mod project_updates;
#[path = "rpc/responses.rs"]
mod responses;
mod room_handlers;
#[path = "rpc/routes.rs"]
mod routes;
mod rpc_core;
mod server_connection;
mod server_lifecycle;
#[path = "rpc/session.rs"]
mod session;
#[path = "rpc/updates.rs"]
mod updates;
mod user_handlers;
#[path = "rpc/utils.rs"]
mod utils;

use call_handlers::*;
use channel_admin::*;
use channel_buffers::{
    channel_buffer_updated, join_channel_buffer, leave_channel_buffer, rejoin_channel_buffers,
    send_notifications, update_channel_buffer,
};
use channel_messages::*;
use channel_subscriptions::*;
use project_sharing::*;
use project_updates::*;
pub use responses::ConnectionGuard;
use responses::{Response, StreamResponse};
use room_handlers::*;
pub use routes::routes;
use rpc_core::*;
pub use session::Principal;
use session::{DbHandle, MessageContext, Session};
use updates::*;
use user_handlers::*;
use utils::ResultExt;

use crate::{
    AppState, Error, Result,
    db::{
        self, BufferId, Capability, Channel, ChannelId, ChannelRole, ChannelsForUser, Database,
        InviteMemberResult, MembershipUpdated, NotificationId, ProjectId, RejoinedProject,
        RemoveChannelMemberResult, RespondToChannelInvite, RoomId, ServerId, UserId,
    },
    executor::Executor,
};
use anyhow::{Context as _, anyhow, bail};
use async_tungstenite::tungstenite::{
    Message as TungsteniteMessage, protocol::CloseFrame as TungsteniteCloseFrame,
};
use axum::extract::ws::{CloseFrame as AxumCloseFrame, Message as AxumMessage};
use collections::{HashSet, TypeIdHashMap};
pub use connection_pool::{ConnectionPool, MavVersion};
use futures::TryFutureExt as _;
use rpc::proto::split_repository_update;
use tracing::Span;
use util::paths::PathStyle;

use futures::{
    FutureExt, StreamExt,
    channel::oneshot,
    future::BoxFuture,
    stream::{BoxStream, FuturesUnordered},
};
use rpc::{
    Connection, ConnectionId, ErrorCode, ErrorCodeExt, ErrorExt, Peer, TypedEnvelope,
    proto::{
        self, Ack, AnyTypedEnvelope, EntityMessage, EnvelopedMessage, LiveKitConnectionInfo,
        RequestMessage, ShareProject, UpdateChannelBufferCollaborators,
    },
};
use std::{
    any::TypeId,
    future::Future,
    mem,
    sync::{Arc, atomic::AtomicBool},
    time::{Duration, Instant},
};
use tokio::sync::{Semaphore, watch};
use tracing::{
    Instrument,
    field::{self},
    info_span, instrument,
};

pub const RECONNECT_TIMEOUT: Duration = Duration::from_secs(30);

// kubernetes gives terminated pods 10s to shutdown gracefully. After they're gone, we can clean up old resources.
pub const CLEANUP_TIMEOUT: Duration = Duration::from_secs(15);

const NOTIFICATION_COUNT_PER_PAGE: usize = 50;

const TOTAL_DURATION_MS: &str = "total_duration_ms";
const PROCESSING_DURATION_MS: &str = "processing_duration_ms";
const QUEUE_DURATION_MS: &str = "queue_duration_ms";
const HOST_WAITING_MS: &str = "host_waiting_ms";

type MessageHandler =
    Box<dyn Send + Sync + Fn(Box<dyn AnyTypedEnvelope>, Session, Span) -> BoxFuture<'static, ()>>;

pub struct Server {
    id: parking_lot::Mutex<ServerId>,
    peer: Arc<Peer>,
    pub connection_pool: Arc<parking_lot::Mutex<ConnectionPool>>,
    app_state: Arc<AppState>,
    handlers: TypeIdHashMap<MessageHandler>,
    teardown: watch::Sender<bool>,
}

impl Server {
    pub fn new(id: ServerId, app_state: Arc<AppState>) -> Arc<Self> {
        let mut server = Self {
            id: parking_lot::Mutex::new(id),
            peer: Peer::new(id.0 as u32),
            app_state,
            connection_pool: Default::default(),
            handlers: Default::default(),
            teardown: watch::channel(false).0,
        };

        server
            .add_request_handler(ping)
            .add_request_handler(create_room)
            .add_request_handler(join_room)
            .add_request_handler(rejoin_room)
            .add_request_handler(leave_room)
            .add_request_handler(set_room_participant_role)
            .add_request_handler(call)
            .add_request_handler(cancel_call)
            .add_message_handler(decline_call)
            .add_request_handler(update_participant_location)
            .add_request_handler(share_project)
            .add_message_handler(unshare_project)
            .add_request_handler(join_project)
            .add_message_handler(leave_project)
            .add_request_handler(update_project)
            .add_request_handler(update_worktree)
            .add_request_handler(update_repository)
            .add_request_handler(remove_repository)
            .add_message_handler(start_language_server)
            .add_message_handler(update_language_server)
            .add_message_handler(update_diagnostic_summary)
            .add_message_handler(update_worktree_settings)
            .add_request_handler(forward_read_only_project_request::<proto::FindSearchCandidates>)
            .add_request_handler(forward_read_only_project_request::<proto::GetDocumentHighlights>)
            .add_request_handler(forward_read_only_project_request::<proto::GetDocumentSymbols>)
            .add_request_handler(forward_read_only_project_request::<proto::GetProjectSymbols>)
            .add_request_handler(forward_read_only_project_request::<proto::OpenBufferForSymbol>)
            .add_request_handler(forward_read_only_project_request::<proto::OpenBufferById>)
            .add_request_handler(forward_read_only_project_request::<proto::SynchronizeBuffers>)
            .add_request_handler(forward_read_only_project_request::<proto::ResolveInlayHint>)
            .add_request_handler(forward_read_only_project_request::<proto::ResolveCodeAction>)
            .add_request_handler(forward_read_only_project_request::<proto::ResolveDocumentLink>)
            .add_request_handler(forward_read_only_project_request::<proto::GetColorPresentation>)
            .add_request_handler(forward_read_only_project_request::<proto::OpenBufferByPath>)
            .add_request_handler(forward_read_only_project_request::<proto::OpenImageByPath>)
            .add_request_handler(forward_read_only_project_request::<proto::DownloadFileByPath>)
            .add_request_handler(forward_read_only_project_request::<proto::GitGetBranches>)
            .add_request_handler(forward_read_only_project_request::<proto::GetDefaultBranch>)
            .add_request_handler(forward_read_only_project_request::<proto::OpenUnstagedDiff>)
            .add_request_handler(forward_read_only_project_request::<proto::OpenUncommittedDiff>)
            .add_request_handler(forward_read_only_project_request::<proto::LspExtExpandMacro>)
            .add_request_handler(forward_read_only_project_request::<proto::LspExtOpenDocs>)
            .add_request_handler(forward_mutating_project_request::<proto::LspExtRunnables>)
            .add_request_handler(
                forward_read_only_project_request::<proto::LspExtSwitchSourceHeader>,
            )
            .add_request_handler(forward_read_only_project_request::<proto::LspExtGoToParentModule>)
            .add_request_handler(forward_read_only_project_request::<proto::LspExtCancelFlycheck>)
            .add_request_handler(forward_read_only_project_request::<proto::LspExtRunFlycheck>)
            .add_request_handler(forward_read_only_project_request::<proto::LspExtClearFlycheck>)
            .add_request_handler(
                forward_mutating_project_request::<proto::RegisterBufferWithLanguageServers>,
            )
            .add_request_handler(forward_mutating_project_request::<proto::UpdateGitBranch>)
            .add_request_handler(forward_mutating_project_request::<proto::GetCompletions>)
            .add_request_handler(
                forward_mutating_project_request::<proto::ApplyCompletionAdditionalEdits>,
            )
            .add_request_handler(forward_mutating_project_request::<proto::OpenNewBuffer>)
            .add_request_handler(
                forward_mutating_project_request::<proto::ResolveCompletionDocumentation>,
            )
            .add_request_handler(forward_mutating_project_request::<proto::ApplyCodeAction>)
            .add_request_handler(forward_mutating_project_request::<proto::PrepareRename>)
            .add_request_handler(forward_mutating_project_request::<proto::PerformRename>)
            .add_request_handler(forward_mutating_project_request::<proto::ReloadBuffers>)
            .add_request_handler(forward_mutating_project_request::<proto::ApplyCodeActionKind>)
            .add_request_handler(forward_mutating_project_request::<proto::FormatBuffers>)
            .add_request_handler(forward_mutating_project_request::<proto::CreateProjectEntry>)
            .add_request_handler(forward_mutating_project_request::<proto::RenameProjectEntry>)
            .add_request_handler(forward_mutating_project_request::<proto::CopyProjectEntry>)
            .add_request_handler(forward_mutating_project_request::<proto::DeleteProjectEntry>)
            .add_request_handler(forward_mutating_project_request::<proto::ExpandProjectEntry>)
            .add_request_handler(
                forward_mutating_project_request::<proto::ExpandAllForProjectEntry>,
            )
            .add_request_handler(forward_mutating_project_request::<proto::OnTypeFormatting>)
            .add_request_handler(forward_mutating_project_request::<proto::SaveBuffer>)
            .add_request_handler(forward_mutating_project_request::<proto::BlameBuffer>)
            .add_request_handler(lsp_query)
            .add_message_handler(broadcast_project_message_from_host::<proto::LspQueryResponse>)
            .add_request_handler(forward_mutating_project_request::<proto::RestartLanguageServers>)
            .add_request_handler(forward_mutating_project_request::<proto::StopLanguageServers>)
            .add_request_handler(forward_mutating_project_request::<proto::LinkedEditingRange>)
            .add_message_handler(create_buffer_for_peer)
            .add_message_handler(create_image_for_peer)
            .add_request_handler(update_buffer)
            .add_message_handler(broadcast_project_message_from_host::<proto::RefreshInlayHints>)
            .add_message_handler(
                broadcast_project_message_from_host::<proto::RefreshSemanticTokens>,
            )
            .add_message_handler(broadcast_project_message_from_host::<proto::RefreshCodeLens>)
            .add_message_handler(broadcast_project_message_from_host::<proto::UpdateBufferFile>)
            .add_message_handler(broadcast_project_message_from_host::<proto::BufferReloaded>)
            .add_message_handler(broadcast_project_message_from_host::<proto::BufferSaved>)
            .add_message_handler(broadcast_project_message_from_host::<proto::UpdateDiffBases>)
            .add_message_handler(
                broadcast_project_message_from_host::<proto::PullWorkspaceDiagnostics>,
            )
            .add_request_handler(get_users)
            .add_request_handler(fuzzy_search_users)
            .add_request_handler(request_contact)
            .add_request_handler(remove_contact)
            .add_request_handler(respond_to_contact_request)
            .add_message_handler(subscribe_to_channels)
            .add_request_handler(create_channel)
            .add_request_handler(delete_channel)
            .add_request_handler(invite_channel_member)
            .add_request_handler(remove_channel_member)
            .add_request_handler(set_channel_member_role)
            .add_request_handler(set_channel_visibility)
            .add_request_handler(rename_channel)
            .add_request_handler(join_channel_buffer)
            .add_request_handler(leave_channel_buffer)
            .add_message_handler(update_channel_buffer)
            .add_request_handler(rejoin_channel_buffers)
            .add_request_handler(get_channel_members)
            .add_request_handler(respond_to_channel_invite)
            .add_request_handler(join_channel)
            .add_request_handler(join_channel_chat)
            .add_message_handler(leave_channel_chat)
            .add_request_handler(send_channel_message)
            .add_request_handler(remove_channel_message)
            .add_request_handler(update_channel_message)
            .add_request_handler(get_channel_messages)
            .add_request_handler(get_channel_messages_by_id)
            .add_request_handler(get_notifications)
            .add_request_handler(mark_notification_as_read)
            .add_request_handler(move_channel)
            .add_request_handler(reorder_channel)
            .add_request_handler(follow)
            .add_message_handler(unfollow)
            .add_message_handler(update_followers)
            .add_message_handler(acknowledge_channel_message)
            .add_message_handler(acknowledge_buffer_version)
            .add_request_handler(forward_mutating_project_request::<proto::Stage>)
            .add_request_handler(forward_mutating_project_request::<proto::Unstage>)
            .add_request_handler(forward_mutating_project_request::<proto::Stash>)
            .add_request_handler(forward_mutating_project_request::<proto::StashPop>)
            .add_request_handler(forward_mutating_project_request::<proto::StashDrop>)
            .add_request_handler(forward_mutating_project_request::<proto::Commit>)
            .add_request_handler(forward_mutating_project_request::<proto::RunGitHook>)
            .add_request_handler(forward_mutating_project_request::<proto::GitInit>)
            .add_request_handler(forward_read_only_project_request::<proto::GetRemotes>)
            .add_request_handler(forward_read_only_project_request::<proto::GitShow>)
            .add_request_handler(forward_read_only_project_request::<proto::LoadCommitDiff>)
            .add_request_handler(forward_read_only_project_request::<proto::GitReset>)
            .add_request_handler(forward_read_only_project_request::<proto::GitCheckoutFiles>)
            .add_request_handler(forward_mutating_project_request::<proto::SetIndexText>)
            .add_request_handler(forward_mutating_project_request::<proto::ToggleBreakpoint>)
            .add_message_handler(broadcast_project_message_from_host::<proto::BreakpointsForFile>)
            .add_request_handler(forward_mutating_project_request::<proto::OpenCommitMessageBuffer>)
            .add_request_handler(forward_mutating_project_request::<proto::GitDiff>)
            .add_request_handler(forward_mutating_project_request::<proto::GetTreeDiff>)
            .add_request_handler(forward_mutating_project_request::<proto::GetBlobContent>)
            .add_request_handler(forward_mutating_project_request::<proto::GitCreateBranch>)
            .add_request_handler(forward_mutating_project_request::<proto::GitChangeBranch>)
            .add_request_handler(forward_mutating_project_request::<proto::GitCreateRemote>)
            .add_request_handler(forward_mutating_project_request::<proto::GitRemoveRemote>)
            .add_request_handler(forward_read_only_project_request::<proto::GitGetWorktrees>)
            .add_request_handler(forward_read_only_project_request::<proto::GitWorktreeCreatedAt>)
            .add_request_handler(forward_read_only_project_request::<proto::GitGetHeadSha>)
            .add_request_handler(forward_read_only_project_request::<proto::GetCommitData>)
            .add_request_stream_handler(
                forward_read_only_project_stream_request::<proto::GetInitialGraphData>,
            )
            .add_request_stream_handler(
                forward_read_only_project_stream_request::<proto::SearchCommits>,
            )
            .add_request_handler(forward_mutating_project_request::<proto::GitCreateWorktree>)
            .add_request_handler(disallow_guest_request::<proto::GitRemoveWorktree>)
            .add_request_handler(disallow_guest_request::<proto::GitRenameWorktree>)
            .add_request_handler(forward_mutating_project_request::<proto::GitEditRef>)
            .add_request_handler(forward_mutating_project_request::<proto::GitRepairWorktrees>)
            .add_request_handler(disallow_guest_request::<proto::GitCreateArchiveCheckpoint>)
            .add_request_handler(disallow_guest_request::<proto::GitRestoreArchiveCheckpoint>)
            .add_request_handler(forward_mutating_project_request::<proto::CheckForPushedCommits>)
            .add_request_handler(forward_mutating_project_request::<proto::ToggleLspLogs>)
            .add_message_handler(broadcast_project_message_from_host::<proto::LanguageServerLog>)
            .add_request_handler(forward_project_search_chunk);

        Arc::new(server)
    }
}
