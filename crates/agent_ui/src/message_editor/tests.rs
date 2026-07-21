use std::{ops::Range, path::Path, path::PathBuf, sync::Arc};

use acp_thread::MentionUri;
use agent::{ThreadStore, outline};
use agent_client_protocol::schema::v1 as acp;
use base64::Engine as _;
use editor::{
    AnchorRangeExt as _, Editor, EditorMode, MultiBufferOffset, SelectionEffects,
    actions::{Cut, Paste},
};

use fs::FakeFs;
use futures::{FutureExt as _, StreamExt as _};
use gpui::{
    AppContext, ClipboardEntry, ClipboardItem, Entity, EventEmitter, ExternalPaths, FocusHandle,
    Focusable, Task, TestAppContext, VisualTestContext,
};
use language_model::LanguageModelRegistry;
use lsp::{CompletionContext, CompletionTriggerKind};
use parking_lot::RwLock;
use project::{AgentId, CompletionIntent, Project, ProjectPath};
use serde_json::{Value, json};

use text::Point;
use ui::{App, Context, IntoElement, Render, SharedString, Window};
use util::{path, paths::PathStyle, rel_path::rel_path};
use workspace::{AppState, Item, MultiWorkspace, Workspace};

use crate::completion_provider::{AgentContextSelection, AvailableSkill, PromptContextType};
use crate::{
    conversation_view::tests::init_test,
    mention_set::insert_crease_for_mention,
    message_editor::{
        Mention, MessageEditor, MessageEditorEvent, SessionCapabilities, parse_mention_links,
    },
};

mod capabilities_tests;
mod completion_tests;
mod copy_cut_tests;
mod external_path_tests;
mod mention_removal_tests;
mod parse_links_tests;
mod selection_insertion_tests;
mod set_message_tests;
mod thread_and_context_tests;
