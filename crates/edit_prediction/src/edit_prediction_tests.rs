use client::{RefreshLlmTokenListener, UserStore, test::FakeServer};
use clock::FakeSystemClock;
use clock::ReplicaId;
use cloud_api_types::{
    CreateLlmTokenResponse, LlmToken, Organization, OrganizationConfiguration,
    OrganizationEditPredictionConfiguration, OrganizationId, SettledEditPrediction,
    SubmitEditPredictionSettledBatchBody, SubmitEditPredictionSettledResponse,
};
use cloud_llm_client::{
    EditPredictionRejectReason, EditPredictionRejection, PredictEditsRequestTrigger,
    RejectEditPredictionsBody,
    predict_edits_v3::{PredictEditsV3Request, PredictEditsV3Response},
    predict_edits_v4::{PredictEditsV4Request, PredictEditsV4Response},
};
use db::AppDatabase;
use edit_prediction_types::EditPredictionRequestTrigger;
use feature_flags::{FeatureFlag as _, FeatureFlagAppExt as _, FeatureFlagsSettings};
use futures::{
    AsyncReadExt, FutureExt, StreamExt,
    channel::{mpsc, oneshot},
};
use gpui::App;
use gpui::{
    Entity, TestAppContext, UpdateGlobal,
    http_client::{FakeHttpClient, Response},
};
use indoc::indoc;
use language::{
    Anchor, Buffer, Capability, Diagnostic, DiagnosticEntry, DiagnosticSet, DiagnosticSeverity,
    Point, unified_diff_with_offsets,
};
use lsp::LanguageServerId;
use parking_lot::Mutex;
use pretty_assertions::{assert_eq, assert_matches};
use project::{FakeFs, Project};
use serde_json::json;
use settings::EditPredictionDataCollectionChoice;
use settings::SettingsStore;
use std::{ops::Range, path::Path, sync::Arc, time::Duration};
use util::{
    path,
    test::{TextRangeMarker, marked_text_ranges_by},
};
use uuid::Uuid;
use workspace::{AppState, CollaboratorId, MultiWorkspace};
use zeta_prompt::Zeta2PromptInput;

use crate::prediction::EditPredictionInputs;
use crate::udiff::apply_diff_to_string;
use crate::{
    BufferEditPrediction, EDIT_PREDICTION_SETTLED_QUIESCENCE, EditPredictionId,
    EditPredictionStore, REJECT_REQUEST_DEBOUNCE, REQUEST_TIMEOUT_BACKOFF,
};

use super::*;

mod auth_and_settled;
mod collaborator_history;
mod edit_history;
mod empty_and_current;
mod path_fallback;
mod predicted_coalescing;
mod rejections_and_diagnostics;
mod request_cancellation;
mod request_flow;
mod sample_capture;
mod test_harness;
use test_harness::*;

fn render_events_with_predicted(events: &[StoredEvent]) -> Vec<String> {
    events
        .iter()
        .map(|e| {
            let zeta_prompt::Event::BufferChange {
                diff, predicted, ..
            } = e.event.as_ref();
            let prefix = if *predicted { "predicted" } else { "manual" };
            format!("{}\n{}", prefix, diff)
        })
        .collect()
}

fn make_collaborator_replica(
    buffer: &Entity<Buffer>,
    cx: &mut TestAppContext,
) -> (Entity<Buffer>, clock::Global) {
    let (state, version) =
        buffer.read_with(cx, |buffer, _cx| (buffer.to_proto(_cx), buffer.version()));
    let collaborator = cx.new(|_cx| {
        Buffer::from_proto(ReplicaId::new(1), Capability::ReadWrite, state, None).unwrap()
    });
    (collaborator, version)
}

async fn apply_collaborator_edit(
    collaborator: &Entity<Buffer>,
    buffer: &Entity<Buffer>,
    since_version: &mut clock::Global,
    edit_range: Range<usize>,
    new_text: &str,
    cx: &mut TestAppContext,
) {
    collaborator.update(cx, |collaborator, cx| {
        collaborator.edit([(edit_range, new_text)], None, cx);
    });

    let serialize_task = collaborator.read_with(cx, |collaborator, cx| {
        collaborator.serialize_ops(Some(since_version.clone()), cx)
    });
    let ops = serialize_task.await;
    *since_version = collaborator.read_with(cx, |collaborator, _cx| collaborator.version());

    buffer.update(cx, |buffer, cx| {
        buffer.apply_ops(
            ops.into_iter()
                .map(|op| language::proto::deserialize_operation(op).unwrap()),
            cx,
        );
    });
}

mod interpolation;

fn init_test(cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.set_global(AppDatabase::test_new());
        let settings_store = SettingsStore::test(cx);
        cx.set_global(settings_store);
        disable_jumps_feature_flag(cx);
    });
}

fn disable_jumps_feature_flag(cx: &mut App) {
    SettingsStore::update_global(cx, |store, _| {
        store.register_setting::<FeatureFlagsSettings>();
    });
    set_jumps_feature_flag_override(cx, "off");
    cx.update_flags(false, vec![]);
}

fn set_jumps_feature_flag_override(cx: &mut App, value: &str) {
    SettingsStore::update_global(cx, |store, cx| {
        store.update_user_settings(cx, |content| {
            content.feature_flags.get_or_insert_default().insert(
                EditPredictionJumpsFeatureFlag::NAME.to_string(),
                value.to_string(),
            );
        });
    });
}

async fn apply_edit_prediction(
    buffer_content: &str,
    completion_response: &str,
    cx: &mut TestAppContext,
) -> String {
    let fs = project::FakeFs::new(cx.executor());
    let project = Project::test(fs.clone(), [path!("/project").as_ref()], cx).await;
    let buffer = cx.new(|cx| Buffer::local(buffer_content, cx));
    let (ep_store, response) = make_test_ep_store(&project, cx).await;
    *response.lock() = completion_response.to_string();
    let edit_prediction = run_edit_prediction(&buffer, &project, &ep_store, cx).await;
    buffer.update(cx, |buffer, cx| {
        buffer.edit(edit_prediction.edits.iter().cloned(), None, cx)
    });
    buffer.read_with(cx, |buffer, _| buffer.text())
}

async fn run_edit_prediction(
    buffer: &Entity<Buffer>,
    project: &Entity<Project>,
    ep_store: &Entity<EditPredictionStore>,
    cx: &mut TestAppContext,
) -> EditPrediction {
    let cursor = buffer.read_with(cx, |buffer, _| buffer.anchor_before(Point::new(1, 0)));
    ep_store.update(cx, |ep_store, cx| {
        ep_store.register_buffer(buffer, &project, cx)
    });
    cx.background_executor.run_until_parked();
    let prediction_task = ep_store.update(cx, |ep_store, cx| {
        ep_store.request_prediction(
            &project,
            buffer,
            cursor,
            PredictEditsRequestTrigger::Other,
            cx,
        )
    });
    prediction_task.await.unwrap().unwrap().prediction
}

async fn make_test_ep_store(
    project: &Entity<Project>,
    cx: &mut TestAppContext,
) -> (Entity<EditPredictionStore>, Arc<Mutex<String>>) {
    let default_response = "hello world\n".to_string();
    let completion_response: Arc<Mutex<String>> = Arc::new(Mutex::new(default_response));
    let http_client = FakeHttpClient::create({
        let completion_response = completion_response.clone();
        let mut next_request_id = 0;
        move |req| {
            let completion_response = completion_response.clone();
            let method = req.method().clone();
            let uri = req.uri().path().to_string();
            let mut body = req.into_body();
            async move {
                match (method, uri.as_str()) {
                    (Method::POST, "/client/llm_tokens") => Ok(http_client::Response::builder()
                        .status(200)
                        .body(
                            serde_json::to_string(&CreateLlmTokenResponse {
                                token: LlmToken("the-llm-token".to_string()),
                            })
                            .unwrap()
                            .into(),
                        )
                        .unwrap()),
                    (Method::POST, "/predict_edits/v3") => {
                        let mut buf = Vec::new();
                        body.read_to_end(&mut buf).await.ok();
                        let decompressed = zstd::decode_all(&buf[..]).unwrap();
                        let req: PredictEditsV3Request =
                            serde_json::from_slice(&decompressed).unwrap();

                        next_request_id += 1;
                        Ok(http_client::Response::builder()
                            .status(200)
                            .body(
                                serde_json::to_string(&PredictEditsV3Response {
                                    request_id: format!("request-{next_request_id}"),
                                    editable_range: 0..req.input.cursor_excerpt.len(),
                                    output: completion_response.lock().clone(),
                                    model_version: None,
                                    cursor_offset: None,
                                })
                                .unwrap()
                                .into(),
                            )
                            .unwrap())
                    }
                    (Method::POST, "/predict_edits/v4") => {
                        let mut buf = Vec::new();
                        body.read_to_end(&mut buf).await.ok();
                        let decompressed = zstd::decode_all(&buf[..]).unwrap();
                        let req: PredictEditsV4Request =
                            serde_json::from_slice(&decompressed).unwrap();

                        next_request_id += 1;
                        let output = completion_response.lock().clone();
                        let file = req
                            .input
                            .editable_context
                            .iter()
                            .find(|file| file.path == req.input.cursor_path)
                            .or_else(|| req.input.editable_context.first())
                            .expect("V4 requests should include editable context");
                        let excerpt = file
                            .excerpts
                            .first()
                            .expect("V4 editable context should include an excerpt");
                        let diff = unified_diff_with_offsets(
                            &excerpt.text,
                            &output,
                            excerpt.row_range.start,
                            excerpt.row_range.start,
                        );
                        let path = file.path.to_string_lossy();
                        let response = PredictEditsV4Response {
                            request_id: format!("request-{next_request_id}"),
                            patch: format!("--- a/{path}\n+++ b/{path}\n{diff}"),
                            model_version: None,
                        };
                        Ok(http_client::Response::builder()
                            .status(200)
                            .body(serde_json::to_string(&response).unwrap().into())
                            .unwrap())
                    }
                    _ => Ok(http_client::Response::builder()
                        .status(404)
                        .body("Not Found".to_string().into())
                        .unwrap()),
                }
            }
        }
    });

    let client = cx.update(|cx| Client::new(Arc::new(FakeSystemClock::new()), http_client, cx));
    let user_store = cx.update(|cx| cx.new(|cx| client::UserStore::new(client.clone(), cx)));
    cx.update(|cx| {
        RefreshLlmTokenListener::register(client.clone(), user_store.clone(), cx);
    });
    let _server = FakeServer::for_client(42, &client, cx).await;

    let project_user_store = cx.update(|cx| project.read(cx).user_store());
    set_test_organization(&project_user_store, cx);

    let ep_store = cx.new(|cx| {
        let mut ep_store = EditPredictionStore::new(client, project.read(cx).user_store(), cx);
        ep_store.set_edit_prediction_model(EditPredictionModel::Zeta);

        let worktrees = project.read(cx).worktrees(cx).collect::<Vec<_>>();
        for worktree in worktrees {
            let worktree_id = worktree.read(cx).id();
            ep_store
                .get_or_init_project(project, cx)
                .license_detection_watchers
                .entry(worktree_id)
                .or_insert_with(|| Rc::new(LicenseDetectionWatcher::new(&worktree, cx)));
        }

        ep_store
    });

    (ep_store, completion_response)
}

fn to_completion_edits(
    iterator: impl IntoIterator<Item = (Range<usize>, Arc<str>)>,
    buffer: &Entity<Buffer>,
    cx: &App,
) -> Vec<(Range<Anchor>, Arc<str>)> {
    let buffer = buffer.read(cx);
    iterator
        .into_iter()
        .map(|(range, text)| {
            (
                buffer.anchor_after(range.start)..buffer.anchor_before(range.end),
                text,
            )
        })
        .collect()
}

fn from_completion_edits(
    editor_edits: &[(Range<Anchor>, Arc<str>)],
    buffer: &Entity<Buffer>,
    cx: &App,
) -> Vec<(Range<usize>, Arc<str>)> {
    let buffer = buffer.read(cx);
    editor_edits
        .iter()
        .map(|(range, text)| {
            (
                range.start.to_offset(buffer)..range.end.to_offset(buffer),
                text.clone(),
            )
        })
        .collect()
}

mod data_collection;

#[ctor::ctor(unsafe)]
fn init_logger() {
    zlog::init_test();
}
