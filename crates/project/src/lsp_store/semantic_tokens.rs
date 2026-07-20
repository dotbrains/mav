mod buffer_conversion;
mod raw_tokens;
mod stylizer;
mod token_types;

use std::{collections::hash_map, sync::Arc};

use anyhow::Result;

use clock::Global;
use collections::HashMap;
use futures::{
    FutureExt as _,
    future::{Shared, join_all},
};
use gpui::{App, AppContext, AsyncApp, Context, Entity, ReadGlobal as _, SharedString, Task};
use language::{Buffer, LanguageName, language_settings::all_language_settings};
use lsp::{AdapterServerCapabilities, LanguageServerId};
use rpc::{TypedEnvelope, proto};
use settings::{SemanticTokenRules, Settings as _, SettingsStore};
use util::ResultExt as _;

use crate::{
    LanguageServerToQuery, LspStore, LspStoreEvent,
    lsp_command::{LspCommand, SemanticTokensDelta, SemanticTokensFull, SemanticTokensResponse},
    project_settings::ProjectSettings,
};

use self::buffer_conversion::raw_to_buffer_semantic_tokens;
#[cfg(test)]
use self::raw_tokens::SemanticToken;
use self::raw_tokens::{RawSemanticTokens, ServerSemanticTokens};
pub use self::stylizer::SemanticTokenStylizer;
pub use self::token_types::{BufferSemanticToken, BufferSemanticTokens, TokenType};

pub(super) struct SemanticTokenConfig {
    stylizers: HashMap<(LanguageServerId, Option<LanguageName>), SemanticTokenStylizer>,
    rules: SemanticTokenRules,
    global_mode: settings::SemanticTokens,
}

impl SemanticTokenConfig {
    pub(super) fn new(cx: &App) -> Self {
        Self {
            stylizers: HashMap::default(),
            rules: ProjectSettings::get_global(cx)
                .global_lsp_settings
                .semantic_token_rules
                .clone(),
            global_mode: all_language_settings(None, cx).defaults.semantic_tokens,
        }
    }

    pub(super) fn remove_server_data(&mut self, server_id: LanguageServerId) {
        self.stylizers.retain(|&(id, _), _| id != server_id);
    }

    pub(super) fn update_rules(&mut self, new_rules: SemanticTokenRules) -> bool {
        if new_rules != self.rules {
            self.rules = new_rules;
            self.stylizers.clear();
            true
        } else {
            false
        }
    }

    /// Clears all cached stylizers.
    ///
    /// This is called when settings change to ensure that any modifications to
    /// language-specific semantic token rules (e.g. from extension install/uninstall)
    /// are picked up. Stylizers are recreated lazily on next use.
    pub(super) fn clear_stylizers(&mut self) {
        self.stylizers.clear();
    }

    pub(super) fn update_global_mode(&mut self, new_mode: settings::SemanticTokens) -> bool {
        if new_mode != self.global_mode {
            self.global_mode = new_mode;
            true
        } else {
            false
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RefreshForServer {
    pub server_id: LanguageServerId,
    pub request_id: Option<usize>,
}

impl LspStore {
    pub fn semantic_tokens(
        &mut self,
        buffer: Entity<Buffer>,
        refresh: Option<RefreshForServer>,
        cx: &mut Context<Self>,
    ) -> SemanticTokensTask {
        let version_queried_for = buffer.read(cx).version();
        let latest_lsp_data = self.latest_lsp_data(&buffer, cx);
        let semantic_tokens_data = latest_lsp_data.semantic_tokens.get_or_insert_default();
        if let Some(refresh) = refresh {
            let mut invalidate_cache = true;
            match semantic_tokens_data
                .latest_invalidation_requests
                .entry(refresh.server_id)
            {
                hash_map::Entry::Occupied(mut o) => {
                    if refresh.request_id > *o.get() {
                        o.insert(refresh.request_id);
                    } else {
                        invalidate_cache = false;
                    }
                }
                hash_map::Entry::Vacant(v) => {
                    v.insert(refresh.request_id);
                }
            }

            if invalidate_cache {
                let SemanticTokensData {
                    raw_tokens,
                    latest_invalidation_requests: _,
                    update,
                } = semantic_tokens_data;
                *update = None;
                raw_tokens.servers.clear();
            }
        }

        if let Some((updating_for, task)) = &semantic_tokens_data.update
            && !version_queried_for.changed_since(updating_for)
        {
            return task.clone();
        }

        let new_tokens = self.fetch_semantic_tokens_for_buffer(
            &buffer,
            refresh.map(|refresh| refresh.server_id),
            cx,
        );

        let task_buffer = buffer.clone();
        let task_version_queried_for = version_queried_for.clone();
        let task = cx
            .spawn(async move |lsp_store, cx| {
                let buffer = task_buffer;
                let version_queried_for = task_version_queried_for;
                let res = if let Some(new_tokens) = new_tokens.await {
                    let (raw_tokens, buffer_snapshot) = lsp_store
                        .update(cx, |lsp_store, cx| {
                            let lsp_data = lsp_store.latest_lsp_data(&buffer, cx);
                            let semantic_tokens_data =
                                lsp_data.semantic_tokens.get_or_insert_default();

                            if version_queried_for == lsp_data.buffer_version {
                                for (server_id, new_tokens_response) in new_tokens {
                                    match new_tokens_response {
                                        SemanticTokensResponse::Full { data, result_id } => {
                                            semantic_tokens_data.raw_tokens.servers.insert(
                                                server_id,
                                                Arc::new(ServerSemanticTokens::from_full(
                                                    data, result_id,
                                                )),
                                            );
                                        }
                                        SemanticTokensResponse::Delta { edits, result_id } => {
                                            if let Some(tokens) = semantic_tokens_data
                                                .raw_tokens
                                                .servers
                                                .get_mut(&server_id)
                                            {
                                                let tokens = Arc::make_mut(tokens);
                                                tokens.result_id = result_id;
                                                tokens.apply(&edits);
                                            }
                                        }
                                    }
                                }
                            }
                            let buffer_snapshot =
                                buffer.read_with(cx, |buffer, _| buffer.snapshot());
                            (semantic_tokens_data.raw_tokens.clone(), buffer_snapshot)
                        })
                        .map_err(Arc::new)?;
                    Some(
                        cx.background_spawn(raw_to_buffer_semantic_tokens(
                            raw_tokens,
                            buffer_snapshot.text.clone(),
                        ))
                        .await,
                    )
                } else {
                    lsp_store.update(cx, |lsp_store, cx| {
                        if let Some(current_lsp_data) =
                            lsp_store.current_lsp_data(buffer.read(cx).remote_id())
                        {
                            if current_lsp_data.buffer_version == version_queried_for {
                                current_lsp_data.semantic_tokens = None;
                            }
                        }
                    })?;
                    None
                };
                Ok(BufferSemanticTokens { tokens: res })
            })
            .shared();

        self.latest_lsp_data(&buffer, cx)
            .semantic_tokens
            .get_or_insert_default()
            .update = Some((version_queried_for, task.clone()));

        task
    }

    pub(super) fn fetch_semantic_tokens_for_buffer(
        &mut self,
        buffer: &Entity<Buffer>,
        for_server: Option<LanguageServerId>,
        cx: &mut Context<Self>,
    ) -> Task<Option<HashMap<LanguageServerId, SemanticTokensResponse>>> {
        if let Some((client, upstream_project_id)) = self.upstream_client() {
            let request = SemanticTokensFull { for_server };
            if !self.is_capable_for_proto_request(buffer, &request, cx) {
                return Task::ready(None);
            }

            let request_timeout = ProjectSettings::get_global(cx)
                .global_lsp_settings
                .get_request_timeout();
            let request_task = client.request_lsp(
                upstream_project_id,
                None,
                request_timeout,
                cx.background_executor().clone(),
                request.to_proto(upstream_project_id, buffer.read(cx)),
            );
            let buffer = buffer.clone();
            cx.spawn(async move |weak_lsp_store, cx| {
                let lsp_store = weak_lsp_store.upgrade()?;
                let tokens = join_all(
                    request_task
                        .await
                        .log_err()
                        .flatten()
                        .map(|response| response.payload)
                        .unwrap_or_default()
                        .into_iter()
                        .map(|response| {
                            let server_id = LanguageServerId::from_proto(response.server_id);
                            let response = request.response_from_proto(
                                response.response,
                                lsp_store.clone(),
                                buffer.clone(),
                                cx.clone(),
                            );
                            async move {
                                match response.await {
                                    Ok(tokens) => Some((server_id, tokens)),
                                    Err(e) => {
                                        log::error!("Failed to query remote semantic tokens for server {server_id:?}: {e:#}");
                                        None
                                    }
                                }
                            }
                        }),
                )
                .await
                .into_iter()
                .flatten()
                .collect();
                Some(tokens)
            })
        } else {
            let token_tasks = self
                .local_lsp_servers_for_buffer(&buffer, cx)
                .into_iter()
                .filter(|&server_id| {
                    for_server.is_none_or(|for_server_id| for_server_id == server_id)
                })
                .filter_map(|server_id| {
                    let capabilities = AdapterServerCapabilities {
                        server_capabilities: self.lsp_server_capabilities.get(&server_id)?.clone(),
                        code_action_kinds: None,
                    };
                    let request_task = match self.semantic_tokens_result_id(server_id, buffer, cx) {
                        Some(result_id) => {
                            let delta_request = SemanticTokensDelta {
                                previous_result_id: result_id,
                            };
                            if !delta_request.check_capabilities(capabilities.clone()) {
                                let full_request = SemanticTokensFull {
                                    for_server: Some(server_id),
                                };
                                if !full_request.check_capabilities(capabilities) {
                                    return None;
                                }

                                self.request_lsp(
                                    buffer.clone(),
                                    LanguageServerToQuery::Other(server_id),
                                    full_request,
                                    cx,
                                )
                            } else {
                                self.request_lsp(
                                    buffer.clone(),
                                    LanguageServerToQuery::Other(server_id),
                                    delta_request,
                                    cx,
                                )
                            }
                        }
                        None => {
                            let request = SemanticTokensFull {
                                for_server: Some(server_id),
                            };
                            if !request.check_capabilities(capabilities) {
                                return None;
                            }
                            self.request_lsp(
                                buffer.clone(),
                                LanguageServerToQuery::Other(server_id),
                                request,
                                cx,
                            )
                        }
                    };
                    Some(async move { (server_id, request_task.await) })
                })
                .collect::<Vec<_>>();
            if token_tasks.is_empty() {
                return Task::ready(None);
            }

            cx.background_spawn(async move {
                Some(
                    join_all(token_tasks)
                        .await
                        .into_iter()
                        .flat_map(|(server_id, response)| {
                            match response {
                                Ok(tokens) => Some((server_id, tokens)),
                                Err(e) => {
                                    log::error!("Failed to query remote semantic tokens for server {server_id:?}: {e:#}");
                                    None
                                }
                            }
                        })
                        .collect()
                )
            })
        }
    }

    /// `request_id` orders per-server refreshes (a higher id invalidates the cache).
    /// Client-initiated refreshes (e.g. after dynamic registration) pass `None`.
    pub(crate) fn refresh_semantic_tokens(
        &mut self,
        server_id: LanguageServerId,
        request_id: Option<usize>,
        cx: &mut Context<Self>,
    ) {
        cx.emit(LspStoreEvent::RefreshSemanticTokens {
            server_id,
            request_id,
        });
        if let Some((client, project_id)) = self.downstream_client.as_ref() {
            client
                .send(proto::RefreshSemanticTokens {
                    project_id: *project_id,
                    server_id: server_id.to_proto(),
                    request_id: request_id.map(|id| id as u64),
                })
                .log_err();
        }
    }

    pub(crate) async fn handle_refresh_semantic_tokens(
        lsp_store: Entity<Self>,
        envelope: TypedEnvelope<proto::RefreshSemanticTokens>,
        mut cx: AsyncApp,
    ) -> Result<proto::Ack> {
        lsp_store.update(&mut cx, |_, cx| {
            cx.emit(LspStoreEvent::RefreshSemanticTokens {
                server_id: LanguageServerId::from_proto(envelope.payload.server_id),
                request_id: envelope.payload.request_id.map(|id| id as usize),
            });
        });
        Ok(proto::Ack {})
    }

    fn semantic_tokens_result_id(
        &mut self,
        server_id: LanguageServerId,
        buffer: &Entity<Buffer>,
        cx: &mut App,
    ) -> Option<SharedString> {
        self.latest_lsp_data(buffer, cx)
            .semantic_tokens
            .as_ref()?
            .raw_tokens
            .servers
            .get(&server_id)?
            .result_id
            .clone()
    }

    pub fn get_or_create_token_stylizer(
        &mut self,
        server_id: LanguageServerId,
        language: Option<&LanguageName>,
        cx: &mut App,
    ) -> Option<&SemanticTokenStylizer> {
        let stylizer = match self
            .semantic_token_config
            .stylizers
            .entry((server_id, language.cloned()))
        {
            hash_map::Entry::Occupied(o) => o.into_mut(),
            hash_map::Entry::Vacant(v) => {
                let tokens_provider = self
                    .lsp_server_capabilities
                    .get(&server_id)?
                    .semantic_tokens_provider
                    .as_ref()?;
                let legend = match tokens_provider {
                    lsp::SemanticTokensServerCapabilities::SemanticTokensOptions(opts) => {
                        &opts.legend
                    }
                    lsp::SemanticTokensServerCapabilities::SemanticTokensRegistrationOptions(
                        opts,
                    ) => &opts.semantic_tokens_options.legend,
                };
                let language_rules = language.and_then(|language| {
                    SettingsStore::global(cx).language_semantic_token_rules(language.as_ref())
                });
                let stylizer = SemanticTokenStylizer::new(server_id, legend, language_rules, cx);
                v.insert(stylizer)
            }
        };
        Some(stylizer)
    }
}

pub type SemanticTokensTask =
    Shared<Task<std::result::Result<BufferSemanticTokens, Arc<anyhow::Error>>>>;

#[derive(Default, Debug)]
pub struct SemanticTokensData {
    pub(super) raw_tokens: RawSemanticTokens,
    pub(super) latest_invalidation_requests: HashMap<LanguageServerId, Option<usize>>,
    update: Option<(Global, SemanticTokensTask)>,
}

impl SemanticTokensData {
    pub(super) fn remove_server_data(&mut self, server_id: LanguageServerId) {
        self.raw_tokens.servers.remove(&server_id);
        self.latest_invalidation_requests.remove(&server_id);
        self.update = None;
    }
}

#[cfg(test)]
mod tests;
