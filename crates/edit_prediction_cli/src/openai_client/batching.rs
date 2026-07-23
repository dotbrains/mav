use anyhow::Result;
use http_client::HttpClient;
use indoc::indoc;
use open_ai::{
    MessageContent, OPEN_AI_API_URL, Request as OpenAiRequest, RequestMessage,
    Response as OpenAiResponse, batches,
};
use reqwest_client::ReqwestClient;
use sqlez::bindable::{Bind, StaticColumnCount};
use sqlez_macros::sql;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::sync::{Arc, Mutex};

use super::request_serialization::{message_content_to_string, message_role_to_string};

pub struct BatchingOpenAiClient {
    pub(super) connection: Mutex<sqlez::connection::Connection>,
    pub(super) http_client: Arc<dyn HttpClient>,
    pub(super) api_key: String,
}

struct CacheRow {
    request_hash: String,
    request: Option<String>,
    response: Option<String>,
    batch_id: Option<String>,
}

impl StaticColumnCount for CacheRow {
    fn column_count() -> usize {
        4
    }
}

impl Bind for CacheRow {
    fn bind(&self, statement: &sqlez::statement::Statement, start_index: i32) -> Result<i32> {
        let next_index = statement.bind(&self.request_hash, start_index)?;
        let next_index = statement.bind(&self.request, next_index)?;
        let next_index = statement.bind(&self.response, next_index)?;
        let next_index = statement.bind(&self.batch_id, next_index)?;
        Ok(next_index)
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct SerializableRequest {
    model: String,
    max_tokens: u64,
    messages: Vec<SerializableMessage>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct SerializableMessage {
    role: String,
    content: String,
}

impl BatchingOpenAiClient {
    pub(super) fn new(cache_path: &Path) -> Result<Self> {
        let http_client: Arc<dyn http_client::HttpClient> = Arc::new(ReqwestClient::new());
        let api_key = std::env::var("OPENAI_API_KEY")
            .map_err(|_| anyhow::anyhow!("OPENAI_API_KEY environment variable not set"))?;

        let connection = sqlez::connection::Connection::open_file(cache_path.to_str().unwrap());
        let mut statement = sqlez::statement::Statement::prepare(
            &connection,
            indoc! {"
                CREATE TABLE IF NOT EXISTS openai_cache (
                    request_hash TEXT PRIMARY KEY,
                    request TEXT,
                    response TEXT,
                    batch_id TEXT
                );
                "},
        )?;
        statement.exec()?;
        drop(statement);

        Ok(Self {
            connection: Mutex::new(connection),
            http_client,
            api_key,
        })
    }

    pub fn lookup(
        &self,
        model: &str,
        max_tokens: u64,
        messages: &[RequestMessage],
        seed: Option<usize>,
    ) -> Result<Option<OpenAiResponse>> {
        let request_hash_str = Self::request_hash(model, max_tokens, messages, seed);
        let connection = self.connection.lock().unwrap();
        let response: Vec<String> = connection.select_bound(
            &sql!(SELECT response FROM openai_cache WHERE request_hash = ?1 AND response IS NOT NULL;),
        )?(request_hash_str.as_str())?;
        Ok(response
            .into_iter()
            .next()
            .and_then(|text| serde_json::from_str(&text).ok()))
    }

    pub fn mark_for_batch(
        &self,
        model: &str,
        max_tokens: u64,
        messages: &[RequestMessage],
        seed: Option<usize>,
    ) -> Result<()> {
        let request_hash = Self::request_hash(model, max_tokens, messages, seed);

        let serializable_messages: Vec<SerializableMessage> = messages
            .iter()
            .map(|msg| SerializableMessage {
                role: message_role_to_string(msg),
                content: message_content_to_string(msg),
            })
            .collect();

        let serializable_request = SerializableRequest {
            model: model.to_string(),
            max_tokens,
            messages: serializable_messages,
        };

        let request = Some(serde_json::to_string(&serializable_request)?);
        let cache_row = CacheRow {
            request_hash,
            request,
            response: None,
            batch_id: None,
        };
        let connection = self.connection.lock().unwrap();
        connection.exec_bound::<CacheRow>(sql!(
            INSERT OR IGNORE INTO openai_cache(request_hash, request, response, batch_id) VALUES (?, ?, ?, ?)))?(
            cache_row,
        )
    }

    pub(super) async fn generate(
        &self,
        model: &str,
        max_tokens: u64,
        messages: Vec<RequestMessage>,
        seed: Option<usize>,
        cache_only: bool,
    ) -> Result<Option<OpenAiResponse>> {
        let response = self.lookup(model, max_tokens, &messages, seed)?;
        if let Some(response) = response {
            return Ok(Some(response));
        }

        if !cache_only {
            self.mark_for_batch(model, max_tokens, &messages, seed)?;
        }

        Ok(None)
    }

    pub(super) async fn sync_batches(&self) -> Result<()> {
        let _batch_ids = self.upload_pending_requests().await?;
        self.download_finished_batches().await
    }

    pub(super) fn pending_batch_count(&self) -> Result<usize> {
        let connection = self.connection.lock().unwrap();
        let counts: Vec<i32> = connection.select(
            sql!(SELECT COUNT(*) FROM openai_cache WHERE batch_id IS NOT NULL AND response IS NULL),
        )?()?;
        Ok(counts.into_iter().next().unwrap_or(0) as usize)
    }

    async fn upload_pending_requests(&self) -> Result<Vec<String>> {
        const BATCH_CHUNK_SIZE: i32 = 16_000;
        let mut all_batch_ids = Vec::new();
        let mut total_uploaded = 0;

        loop {
            let rows: Vec<(String, String)> = {
                let connection = self.connection.lock().unwrap();
                let q = sql!(
                    SELECT request_hash, request FROM openai_cache
                    WHERE batch_id IS NULL AND response IS NULL
                    LIMIT ?
                );
                connection.select_bound(q)?(BATCH_CHUNK_SIZE)?
            };

            if rows.is_empty() {
                break;
            }

            let request_hashes: Vec<String> = rows.iter().map(|(hash, _)| hash.clone()).collect();

            let mut jsonl_content = String::new();
            for (hash, request_str) in &rows {
                let serializable_request: SerializableRequest =
                    serde_json::from_str(request_str).unwrap();

                let messages: Vec<RequestMessage> = serializable_request
                    .messages
                    .into_iter()
                    .map(|msg| match msg.role.as_str() {
                        "user" => RequestMessage::User {
                            content: MessageContent::Plain(msg.content),
                        },
                        "assistant" => RequestMessage::Assistant {
                            content: Some(MessageContent::Plain(msg.content)),
                            tool_calls: Vec::new(),
                            reasoning_content: None,
                        },
                        "system" => RequestMessage::System {
                            content: MessageContent::Plain(msg.content),
                        },
                        _ => RequestMessage::User {
                            content: MessageContent::Plain(msg.content),
                        },
                    })
                    .collect();

                let request = OpenAiRequest {
                    model: serializable_request.model,
                    messages,
                    stream: false,
                    stream_options: None,
                    max_completion_tokens: Some(serializable_request.max_tokens),
                    max_tokens: None,
                    stop: Vec::new(),
                    temperature: None,
                    tool_choice: None,
                    parallel_tool_calls: None,
                    service_tier: None,
                    tools: Vec::new(),
                    prompt_cache_key: None,
                    reasoning_effort: None,
                };

                let custom_id = format!("req_hash_{}", hash);
                let batch_item = batches::BatchRequestItem::new(custom_id, request);
                let line = batch_item
                    .to_jsonl_line()
                    .map_err(|e| anyhow::anyhow!("Failed to serialize batch item: {:?}", e))?;
                jsonl_content.push_str(&line);
                jsonl_content.push('\n');
            }

            let filename = format!("batch_{}.jsonl", chrono::Utc::now().timestamp());
            let file_obj = batches::upload_batch_file(
                self.http_client.as_ref(),
                OPEN_AI_API_URL,
                &self.api_key,
                &filename,
                jsonl_content.into_bytes(),
            )
            .await
            .map_err(|e| anyhow::anyhow!("Failed to upload batch file: {:?}", e))?;

            let batch = batches::create_batch(
                self.http_client.as_ref(),
                OPEN_AI_API_URL,
                &self.api_key,
                batches::CreateBatchRequest::new(file_obj.id),
            )
            .await
            .map_err(|e| anyhow::anyhow!("Failed to create batch: {:?}", e))?;

            {
                let connection = self.connection.lock().unwrap();
                connection.with_savepoint("batch_upload", || {
                    let q = sql!(UPDATE openai_cache SET batch_id = ? WHERE request_hash = ?);
                    let mut exec = connection.exec_bound::<(&str, &str)>(q)?;
                    for hash in &request_hashes {
                        exec((batch.id.as_str(), hash.as_str()))?;
                    }
                    Ok(())
                })?;
            }

            let batch_len = rows.len();
            total_uploaded += batch_len;
            log::info!(
                "Uploaded batch {} with {} requests ({} total)",
                batch.id,
                batch_len,
                total_uploaded
            );

            all_batch_ids.push(batch.id);
        }

        if !all_batch_ids.is_empty() {
            log::info!(
                "Finished uploading {} batches with {} total requests",
                all_batch_ids.len(),
                total_uploaded
            );
        }

        Ok(all_batch_ids)
    }

    fn request_hash(
        model: &str,
        max_tokens: u64,
        messages: &[RequestMessage],
        seed: Option<usize>,
    ) -> String {
        let mut hasher = std::hash::DefaultHasher::new();
        "openai".hash(&mut hasher);
        model.hash(&mut hasher);
        max_tokens.hash(&mut hasher);
        for msg in messages {
            message_content_to_string(msg).hash(&mut hasher);
        }
        if let Some(seed) = seed {
            seed.hash(&mut hasher);
        }
        let request_hash = hasher.finish();
        format!("{request_hash:016x}")
    }
}
