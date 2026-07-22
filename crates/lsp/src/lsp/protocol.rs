use super::*;

pub(super) const JSON_RPC_VERSION: &str = "2.0";
pub(crate) const CONTENT_LEN_HEADER: &str = "Content-Length: ";

/// The default amount of time to wait while initializing or fetching LSP servers, in seconds.
///
/// Should not be used (in favor of DEFAULT_LSP_REQUEST_TIMEOUT) and is exported solely for use inside ProjectSettings defaults.
pub const DEFAULT_LSP_REQUEST_TIMEOUT_SECS: u64 = 120;
/// A timeout representing the value of [DEFAULT_LSP_REQUEST_TIMEOUT_SECS].
///
/// Should **only be used** in tests and as a fallback when a corresponding config value cannot be obtained!
pub const DEFAULT_LSP_REQUEST_TIMEOUT: Duration =
    Duration::from_secs(DEFAULT_LSP_REQUEST_TIMEOUT_SECS);

/// The shutdown timeout for LSP servers (including Prettier/Copilot).
pub(super) const SERVER_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);

pub(super) type NotificationHandler =
    Box<dyn Send + FnMut(Option<RequestId>, Value, &mut AsyncApp)>;
pub(super) type PendingRespondTasks = Arc<Mutex<HashMap<RequestId, Task<()>>>>;
pub(crate) type ResponseHandler = Box<dyn Send + FnOnce(Result<String, Error>) -> Task<()>>;
pub(crate) type IoHandler = Box<dyn Send + FnMut(IoKind, &str)>;

/// Kind of language server stdio given to an IO handler.
#[derive(Debug, Clone, Copy)]
pub enum IoKind {
    StdOut,
    StdIn,
    StdErr,
}

/// Represents a launchable language server. This can either be a standalone binary or the path
/// to a runtime with arguments to instruct it to launch the actual language server file.
#[derive(Clone, Serialize)]
pub struct LanguageServerBinary {
    pub path: PathBuf,
    pub arguments: Vec<OsString>,
    pub env: Option<HashMap<String, String>>,
}

/// Configures the search (and installation) of language servers.
#[derive(Debug, Clone)]
pub struct LanguageServerBinaryOptions {
    /// Whether the adapter should look at the users system
    pub allow_path_lookup: bool,
    /// Whether the adapter should download its own version
    pub allow_binary_download: bool,
    /// Whether the adapter should download a pre-release version
    pub pre_release: bool,
}

pub(super) struct NotificationSerializer(pub(super) Box<dyn FnOnce() -> String + Send + Sync>);

/// A running language server process.
pub struct LanguageServer {
    pub(super) server_id: LanguageServerId,
    pub(super) next_id: AtomicI32,
    pub(super) outbound_tx: channel::Sender<String>,
    pub(super) notification_tx: channel::Sender<NotificationSerializer>,
    pub(super) name: LanguageServerName,
    pub(super) version: Option<SharedString>,
    pub(super) process_name: Arc<str>,
    pub(super) binary: LanguageServerBinary,
    pub(super) capabilities: RwLock<ServerCapabilities>,
    /// Configuration sent to the server, stored for display in the language server logs
    /// buffer. This is represented as the message sent to the LSP in order to avoid cloning it (can
    /// be large in cases like sending schemas to the json server).
    pub(super) configuration: Arc<DidChangeConfigurationParams>,
    pub(super) code_action_kinds: Option<Vec<CodeActionKind>>,
    pub(super) notification_handlers: Arc<Mutex<HashMap<&'static str, NotificationHandler>>>,
    pub(super) response_handlers: Arc<Mutex<Option<HashMap<RequestId, ResponseHandler>>>>,
    /// Tasks spawned by `on_custom_request` to compute responses. Tracked so that
    /// incoming `$/cancelRequest` notifications can cancel them by dropping the task.
    pub(super) pending_respond_tasks: PendingRespondTasks,
    pub(super) io_handlers: Arc<Mutex<HashMap<i32, IoHandler>>>,
    pub(super) executor: BackgroundExecutor,
    #[allow(clippy::type_complexity)]
    pub(super) io_tasks: Mutex<Option<(Task<Option<()>>, Task<Option<()>>)>>,
    pub(super) output_done_rx: Mutex<Option<barrier::Receiver>>,
    pub(super) server: Arc<Mutex<Option<Child>>>,
    pub(super) workspace_folders: Option<Arc<Mutex<BTreeSet<Uri>>>>,
    pub(super) root_uri: Uri,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum LanguageServerSelector {
    Id(LanguageServerId),
    Name(LanguageServerName),
}

/// Identifies a running language server.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct LanguageServerId(pub usize);

impl LanguageServerId {
    pub fn from_proto(id: u64) -> Self {
        Self(id as usize)
    }

    pub fn to_proto(self) -> u64 {
        self.0 as u64
    }
}

/// A name of a language server.
#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize, Serialize, JsonSchema,
)]
#[serde(transparent)]
pub struct LanguageServerName(pub SharedString);

impl std::fmt::Display for LanguageServerName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.0, f)
    }
}

impl AsRef<str> for LanguageServerName {
    fn as_ref(&self) -> &str {
        self.0.as_ref()
    }
}

impl AsRef<OsStr> for LanguageServerName {
    fn as_ref(&self) -> &OsStr {
        self.0.as_ref().as_ref()
    }
}

impl LanguageServerName {
    pub const fn new_static(s: &'static str) -> Self {
        Self(SharedString::new_static(s))
    }

    pub fn from_proto(s: String) -> Self {
        Self(s.into())
    }
}

impl<'a> From<&'a str> for LanguageServerName {
    fn from(str: &'a str) -> LanguageServerName {
        LanguageServerName(str.to_string().into())
    }
}

impl PartialEq<str> for LanguageServerName {
    fn eq(&self, other: &str) -> bool {
        self.0 == other
    }
}

/// Handle to a language server RPC activity subscription.
pub enum Subscription {
    Notification {
        method: &'static str,
        notification_handlers: Option<Weak<Mutex<HashMap<&'static str, NotificationHandler>>>>,
    },
    Io {
        id: i32,
        io_handlers: Option<Weak<Mutex<HashMap<i32, IoHandler>>>>,
    },
}

/// Language server protocol RPC request message ID.
///
/// [LSP Specification](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#requestMessage)
#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RequestId {
    Int(i32),
    Str(String),
}

fn is_unit<T: 'static>(_: &T) -> bool {
    TypeId::of::<T>() == TypeId::of::<()>()
}

/// Language server protocol RPC request message.
///
/// [LSP Specification](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#requestMessage)
#[derive(Serialize, Deserialize)]
pub struct Request<'a, T>
where
    T: 'static,
{
    pub(super) jsonrpc: &'static str,
    pub(crate) id: RequestId,
    pub(super) method: &'a str,
    #[serde(default, skip_serializing_if = "is_unit")]
    pub(super) params: T,
}

/// Language server protocol RPC request response message before it is deserialized into a concrete type.
#[derive(Serialize, Deserialize)]
pub(crate) struct AnyResponse<'a> {
    pub(super) jsonrpc: &'a str,
    pub(crate) id: RequestId,
    #[serde(default)]
    pub(crate) error: Option<Error>,
    #[serde(borrow)]
    pub(crate) result: Option<&'a RawValue>,
}

/// Language server protocol RPC request response message.
///
/// [LSP Specification](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#responseMessage)
#[derive(Serialize)]
pub(super) struct Response<T> {
    pub(super) jsonrpc: &'static str,
    pub(crate) id: RequestId,
    #[serde(flatten)]
    pub(super) value: LspResult<T>,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum LspResult<T> {
    #[serde(rename = "result")]
    Ok(Option<T>),
    Error(Option<Error>),
}

/// Language server protocol RPC notification message.
///
/// [LSP Specification](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#notificationMessage)
#[derive(Serialize, Deserialize)]
pub(super) struct Notification<'a, T>
where
    T: 'static,
{
    pub(super) jsonrpc: &'static str,
    #[serde(borrow)]
    pub(super) method: &'a str,
    #[serde(default, skip_serializing_if = "is_unit")]
    pub(super) params: T,
}

/// Language server RPC notification message before it is deserialized into a concrete type.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct NotificationOrRequest {
    #[serde(default)]
    pub(super) id: Option<RequestId>,
    pub(super) method: String,
    #[serde(default)]
    pub(super) params: Option<Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Error {
    pub(super) code: i64,
    pub(super) message: String,
    #[serde(default)]
    pub(super) data: Option<serde_json::Value>,
}

pub trait LspRequestFuture<O>: Future<Output = ConnectionResult<O>> {
    fn id(&self) -> i32;
}

pub(super) struct LspRequest<F> {
    pub(crate) id: i32,
    request: F,
}

impl<F> LspRequest<F> {
    pub fn new(id: i32, request: F) -> Self {
        Self { id, request }
    }
}

impl<F: Future> Future for LspRequest<F> {
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        // SAFETY: This is standard pin projection, we're pinned so our fields must be pinned.
        let inner = unsafe { Pin::new_unchecked(&mut self.get_unchecked_mut().request) };
        inner.poll(cx)
    }
}

impl<F, O> LspRequestFuture<O> for LspRequest<F>
where
    F: Future<Output = ConnectionResult<O>>,
{
    fn id(&self) -> i32 {
        self.id
    }
}

/// Combined capabilities of the server and the adapter.
#[derive(Debug, Clone)]
pub struct AdapterServerCapabilities {
    // Reported capabilities by the server
    pub server_capabilities: ServerCapabilities,
    // List of code actions supported by the LspAdapter matching the server
    pub code_action_kinds: Option<Vec<CodeActionKind>>,
}

// See the VSCode docs [1] and the LSP Spec [2]
//
// [1]: https://code.visualstudio.com/api/language-extensions/semantic-highlight-guide#standard-token-types-and-modifiers
// [2]: https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#semanticTokenTypes
pub const SEMANTIC_TOKEN_TYPES: &[SemanticTokenType] = &[
    SemanticTokenType::NAMESPACE,
    SemanticTokenType::CLASS,
    SemanticTokenType::ENUM,
    SemanticTokenType::INTERFACE,
    SemanticTokenType::STRUCT,
    SemanticTokenType::TYPE_PARAMETER,
    SemanticTokenType::TYPE,
    SemanticTokenType::PARAMETER,
    SemanticTokenType::VARIABLE,
    SemanticTokenType::PROPERTY,
    SemanticTokenType::ENUM_MEMBER,
    SemanticTokenType::DECORATOR,
    SemanticTokenType::FUNCTION,
    SemanticTokenType::METHOD,
    SemanticTokenType::MACRO,
    SemanticTokenType::new("label"), // Not in the spec, but in the docs.
    SemanticTokenType::COMMENT,
    SemanticTokenType::STRING,
    SemanticTokenType::KEYWORD,
    SemanticTokenType::NUMBER,
    SemanticTokenType::REGEXP,
    SemanticTokenType::OPERATOR,
    SemanticTokenType::MODIFIER, // Only in the spec, not in the docs.
    // Language specific things below.
    // C#
    SemanticTokenType::EVENT,
    // Rust
    SemanticTokenType::new("lifetime"),
];
pub const SEMANTIC_TOKEN_MODIFIERS: &[SemanticTokenModifier] = &[
    SemanticTokenModifier::DECLARATION,
    SemanticTokenModifier::DEFINITION,
    SemanticTokenModifier::READONLY,
    SemanticTokenModifier::STATIC,
    SemanticTokenModifier::DEPRECATED,
    SemanticTokenModifier::ABSTRACT,
    SemanticTokenModifier::ASYNC,
    SemanticTokenModifier::MODIFICATION,
    SemanticTokenModifier::DOCUMENTATION,
    SemanticTokenModifier::DEFAULT_LIBRARY,
    // Language specific things below.
    // Rust
    SemanticTokenModifier::new("constant"),
];
