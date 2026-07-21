use super::*;

/// A channel-based wrapper that delivers tool input to a running tool.
///
/// For non-streaming tools, created via `ToolInput::ready()` so `.recv()` resolves immediately.
/// For streaming tools, partial JSON snapshots arrive via `.recv_partial()` as the LLM streams
/// them, followed by the final complete input available through `.recv()`.
pub struct ToolInput<T> {
    rx: mpsc::UnboundedReceiver<ToolInputPayload<serde_json::Value>>,
    _phantom: PhantomData<T>,
}

impl<T: DeserializeOwned> ToolInput<T> {
    #[cfg(any(test, feature = "test-support"))]
    pub fn resolved(input: impl Serialize) -> Self {
        let value = serde_json::to_value(input).expect("failed to serialize tool input");
        Self::ready(value)
    }

    pub fn ready(value: serde_json::Value) -> Self {
        let (tx, rx) = mpsc::unbounded();
        tx.unbounded_send(ToolInputPayload::Full(value)).ok();
        Self {
            rx,
            _phantom: PhantomData,
        }
    }

    pub fn invalid_json(error_message: String) -> Self {
        let (tx, rx) = mpsc::unbounded();
        tx.unbounded_send(ToolInputPayload::InvalidJson { error_message })
            .ok();
        Self {
            rx,
            _phantom: PhantomData,
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn test() -> (ToolInputSender, Self) {
        let (sender, input) = ToolInputSender::channel();
        (sender, input.cast())
    }

    /// Wait for the final deserialized input, ignoring all partial updates.
    /// Non-streaming tools can use this to wait until the whole input is available.
    pub async fn recv(mut self) -> Result<T> {
        while let Ok(value) = self.next().await {
            match value {
                ToolInputPayload::Full(value) => return Ok(value),
                ToolInputPayload::Partial(_) => {}
                ToolInputPayload::InvalidJson { error_message } => {
                    return Err(anyhow!(error_message));
                }
            }
        }
        Err(anyhow!("tool input was not fully received"))
    }

    pub async fn next(&mut self) -> Result<ToolInputPayload<T>> {
        let value = self
            .rx
            .next()
            .await
            .ok_or_else(|| anyhow!("tool input was not fully received"))?;

        Ok(match value {
            ToolInputPayload::Partial(payload) => ToolInputPayload::Partial(payload),
            ToolInputPayload::Full(payload) => {
                ToolInputPayload::Full(serde_json::from_value(payload)?)
            }
            ToolInputPayload::InvalidJson { error_message } => {
                ToolInputPayload::InvalidJson { error_message }
            }
        })
    }

    pub(super) fn cast<U: DeserializeOwned>(self) -> ToolInput<U> {
        ToolInput {
            rx: self.rx,
            _phantom: PhantomData,
        }
    }
}

pub enum ToolInputPayload<T> {
    Partial(serde_json::Value),
    Full(T),
    InvalidJson { error_message: String },
}

pub struct ToolInputSender {
    has_received_final: bool,
    tx: mpsc::UnboundedSender<ToolInputPayload<serde_json::Value>>,
}

impl ToolInputSender {
    pub(crate) fn channel() -> (Self, ToolInput<serde_json::Value>) {
        let (tx, rx) = mpsc::unbounded();
        let sender = Self {
            tx,
            has_received_final: false,
        };
        let input = ToolInput {
            rx,
            _phantom: PhantomData,
        };
        (sender, input)
    }

    pub(crate) fn has_received_final(&self) -> bool {
        self.has_received_final
    }

    pub fn send_partial(&mut self, payload: serde_json::Value) {
        self.tx
            .unbounded_send(ToolInputPayload::Partial(payload))
            .ok();
    }

    pub fn send_full(&mut self, payload: serde_json::Value) {
        self.has_received_final = true;
        self.tx.unbounded_send(ToolInputPayload::Full(payload)).ok();
    }

    pub fn send_invalid_json(&mut self, error_message: String) {
        self.has_received_final = true;
        self.tx
            .unbounded_send(ToolInputPayload::InvalidJson { error_message })
            .ok();
    }
}
