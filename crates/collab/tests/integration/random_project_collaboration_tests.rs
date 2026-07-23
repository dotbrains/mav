use crate::{RandomizedTest, TestClient, TestError, TestServer, UserTestPlan, run_randomized_test};
use anyhow::{Context as _, Result};
use async_trait::async_trait;
use call::ActiveCall;
use collab::db::UserId;
use collections::{BTreeMap, HashMap};
use editor::Bias;
use fs::{FakeFs, Fs as _};
use git::status::{FileStatus, StatusCode, TrackedStatus, UnmergedStatus, UnmergedStatusCode};
use gpui::{BackgroundExecutor, Entity, TaskExt, TestAppContext};
use language::{
    FakeLspAdapter, Language, LanguageConfig, LanguageMatcher, PointUtf16, range_to_lsp,
};
use lsp::FakeLanguageServer;
use pretty_assertions::assert_eq;
use project::{
    DEFAULT_COMPLETION_CONTEXT, Project, ProjectPath, search::SearchQuery, search::SearchResult,
};
use rand::{
    distr::{self, SampleString},
    prelude::*,
};
use serde::{Deserialize, Serialize};
use std::{
    ops::{Deref, Range},
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
};
use util::{
    ResultExt, path,
    paths::PathStyle,
    rel_path::{RelPath, RelPathBuf, rel_path},
};

#[gpui::test(iterations = 100, on_failure = "crate::save_randomized_test_plan")]
mod git_operations;
mod helpers;
mod lifecycle;
mod operation_application;
mod operation_generation;

use helpers::*;

async fn test_random_project_collaboration(
    cx: &mut TestAppContext,
    executor: BackgroundExecutor,
    rng: StdRng,
) {
    run_randomized_test::<ProjectCollaborationTest>(cx, executor, rng).await;
}

#[derive(Clone, Debug, Serialize, Deserialize)]

enum ClientOperation {
    AcceptIncomingCall,
    RejectIncomingCall,
    LeaveCall,
    InviteContactToCall {
        user_id: UserId,
    },
    OpenLocalProject {
        first_root_name: String,
    },
    OpenRemoteProject {
        host_id: UserId,
        first_root_name: String,
    },
    AddWorktreeToProject {
        project_root_name: String,
        new_root_path: PathBuf,
    },
    CloseRemoteProject {
        project_root_name: String,
    },
    OpenBuffer {
        project_root_name: String,
        is_local: bool,
        full_path: RelPathBuf,
    },
    SearchProject {
        project_root_name: String,
        is_local: bool,
        query: String,
        detach: bool,
    },
    EditBuffer {
        project_root_name: String,
        is_local: bool,
        full_path: RelPathBuf,
        edits: Vec<(Range<usize>, Arc<str>)>,
    },
    CloseBuffer {
        project_root_name: String,
        is_local: bool,
        full_path: RelPathBuf,
    },
    SaveBuffer {
        project_root_name: String,
        is_local: bool,
        full_path: RelPathBuf,
        detach: bool,
    },
    RequestLspDataInBuffer {
        project_root_name: String,
        is_local: bool,
        full_path: RelPathBuf,
        offset: usize,
        kind: LspRequestKind,
        detach: bool,
    },
    CreateWorktreeEntry {
        project_root_name: String,
        is_local: bool,
        full_path: RelPathBuf,
        is_dir: bool,
    },
    WriteFsEntry {
        path: PathBuf,
        is_dir: bool,
        content: String,
    },
    GitOperation {
        operation: GitOperation,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
enum GitOperation {
    WriteGitIndex {
        repo_path: PathBuf,
        contents: Vec<(RelPathBuf, String)>,
    },
    WriteGitBranch {
        repo_path: PathBuf,
        new_branch: Option<String>,
    },
    WriteGitStatuses {
        repo_path: PathBuf,
        statuses: Vec<(RelPathBuf, FileStatus)>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
enum LspRequestKind {
    Rename,
    Completion,
    CodeAction,
    Definition,
    Highlights,
}

struct ProjectCollaborationTest;

#[async_trait(?Send)]
impl RandomizedTest for ProjectCollaborationTest {
    type Operation = ClientOperation;

    async fn initialize(server: &mut TestServer, users: &[UserTestPlan]) {
        let db = &server.app_state.db;
        for (ix, user_a) in users.iter().enumerate() {
            for user_b in &users[ix + 1..] {
                db.send_contact_request(user_a.user_id, user_b.user_id)
                    .await
                    .unwrap();
                db.respond_to_contact_request(user_b.user_id, user_a.user_id, true)
                    .await
                    .unwrap();
            }
        }
    }

    fn generate_operation(
        client: &TestClient,
        rng: &mut StdRng,
        plan: &mut UserTestPlan,
        cx: &TestAppContext,
    ) -> ClientOperation {
        operation_generation::generate_operation(client, rng, plan, cx)
    }

    async fn apply_operation(
        client: &TestClient,
        operation: ClientOperation,
        cx: &mut TestAppContext,
    ) -> Result<(), TestError> {
        operation_application::apply_operation(client, operation, cx).await
    }

    async fn on_client_added(client: &Rc<TestClient>, cx: &mut TestAppContext) {
        lifecycle::on_client_added(client, cx).await
    }

    async fn on_quiesce(server: &mut TestServer, clients: &mut [(Rc<TestClient>, TestAppContext)]) {
        lifecycle::on_quiesce(server, clients).await
    }
}
