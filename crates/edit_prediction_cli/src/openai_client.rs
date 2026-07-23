mod batch_results;
mod batching;
mod request_serialization;

use anyhow::Result;
use http_client::HttpClient;
use open_ai::{
    OPEN_AI_API_URL, Request as OpenAiRequest, RequestMessage, Response as OpenAiResponse,
    non_streaming_completion,
};
use reqwest_client::ReqwestClient;
use std::path::Path;
use std::sync::Arc;

pub struct PlainOpenAiClient {
    pub http_client: Arc<dyn HttpClient>,
    pub api_key: String,
}

impl PlainOpenAiClient {
    pub fn new() -> Result<Self> {
        let http_client: Arc<dyn http_client::HttpClient> = Arc::new(ReqwestClient::new());
        let api_key = std::env::var("OPENAI_API_KEY")
            .map_err(|_| anyhow::anyhow!("OPENAI_API_KEY environment variable not set"))?;
        Ok(Self {
            http_client,
            api_key,
        })
    }

    pub async fn generate(
        &self,
        model: &str,
        max_tokens: u64,
        messages: Vec<RequestMessage>,
    ) -> Result<OpenAiResponse> {
        let request = OpenAiRequest {
            model: model.to_string(),
            messages,
            stream: false,
            stream_options: None,
            max_completion_tokens: Some(max_tokens),
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

        let response = non_streaming_completion(
            self.http_client.as_ref(),
            OPEN_AI_API_URL,
            &self.api_key,
            request,
        )
        .await
        .map_err(|e| anyhow::anyhow!("{:?}", e))?;

        Ok(response)
    }
}

use batching::BatchingOpenAiClient;

pub enum OpenAiClient {
    Plain(PlainOpenAiClient),
    Batch(BatchingOpenAiClient),
    #[allow(dead_code)]
    Dummy,
}

impl OpenAiClient {
    pub fn plain() -> Result<Self> {
        Ok(Self::Plain(PlainOpenAiClient::new()?))
    }

    pub fn batch(cache_path: &Path) -> Result<Self> {
        Ok(Self::Batch(BatchingOpenAiClient::new(cache_path)?))
    }

    #[allow(dead_code)]
    pub fn dummy() -> Self {
        Self::Dummy
    }

    pub async fn generate(
        &self,
        model: &str,
        max_tokens: u64,
        messages: Vec<RequestMessage>,
        seed: Option<usize>,
        cache_only: bool,
    ) -> Result<Option<OpenAiResponse>> {
        match self {
            OpenAiClient::Plain(plain_client) => plain_client
                .generate(model, max_tokens, messages)
                .await
                .map(Some),
            OpenAiClient::Batch(batching_client) => {
                batching_client
                    .generate(model, max_tokens, messages, seed, cache_only)
                    .await
            }
            OpenAiClient::Dummy => panic!("Dummy OpenAI client is not expected to be used"),
        }
    }

    pub async fn sync_batches(&self) -> Result<()> {
        match self {
            OpenAiClient::Plain(_) => Ok(()),
            OpenAiClient::Batch(batching_client) => batching_client.sync_batches().await,
            OpenAiClient::Dummy => panic!("Dummy OpenAI client is not expected to be used"),
        }
    }

    pub fn pending_batch_count(&self) -> Result<usize> {
        match self {
            OpenAiClient::Plain(_) => Ok(0),
            OpenAiClient::Batch(batching_client) => batching_client.pending_batch_count(),
            OpenAiClient::Dummy => panic!("Dummy OpenAI client is not expected to be used"),
        }
    }

    pub async fn import_batches(&self, batch_ids: &[String]) -> Result<()> {
        match self {
            OpenAiClient::Plain(_) => {
                anyhow::bail!("Import batches is only supported with batching client")
            }
            OpenAiClient::Batch(batching_client) => batching_client.import_batches(batch_ids).await,
            OpenAiClient::Dummy => panic!("Dummy OpenAI client is not expected to be used"),
        }
    }
}
