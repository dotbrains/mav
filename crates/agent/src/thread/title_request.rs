use super::*;

pub fn build_thread_title_request(
    messages: &[Arc<Message>],
    temperature: Option<f32>,
) -> LanguageModelRequest {
    let mut request = LanguageModelRequest {
        intent: Some(CompletionIntent::ThreadSummarization),
        temperature,
        ..Default::default()
    };
    extend_request_history_until(messages, &mut request.messages, messages.len());
    request.messages.push(LanguageModelRequestMessage {
        role: Role::User,
        content: vec![SUMMARIZE_THREAD_PROMPT.into()],
        cache: false,
        reasoning_details: None,
    });
    request
}

pub async fn stream_thread_title(
    model: Arc<dyn LanguageModel>,
    request: LanguageModelRequest,
    cx: &AsyncApp,
) -> Result<String> {
    let mut title = String::new();
    let mut events = model.stream_completion(request, cx).await?;
    while let Some(event) = events.next().await {
        let LanguageModelCompletionEvent::Text(text) = event? else {
            continue;
        };
        if let Some(newline_ix) = text.find(|ch| ch == '\n' || ch == '\r') {
            title.push_str(&text[..newline_ix]);
            break;
        }
        title.push_str(&text);
    }
    Ok(title)
}

pub struct TokenUsageUpdated(pub Option<acp_thread::TokenUsage>);

impl EventEmitter<TokenUsageUpdated> for Thread {}

pub struct TitleUpdated;

impl EventEmitter<TitleUpdated> for Thread {}
