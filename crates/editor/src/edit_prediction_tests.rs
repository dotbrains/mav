use edit_prediction_types::{
    EditPredictionDelegate, EditPredictionIconSet, EditPredictionRequestTrigger,
    PredictedCursorPosition,
};
use futures::StreamExt;
use gpui::{
    Entity, Focusable, KeyBinding, KeybindingKeystroke, Keystroke, Modifiers, NoAction, Pixels,
    Task, prelude::*, size,
};
use indoc::indoc;
use language::EditPredictionsMode;
use language::{Buffer, CodeLabel};
use multi_buffer::{Anchor, MultiBufferSnapshot, ToPoint};
use project::{Completion, CompletionResponse, CompletionSource};
use std::{
    ops::Range,
    path::PathBuf,
    rc::Rc,
    sync::{
        Arc,
        atomic::{self, AtomicUsize},
    },
};
use text::{Point, ToOffset};
use ui::prelude::*;

use crate::{
    AcceptEditPrediction, CodeContextMenu, CompletionContext, CompletionProvider, EditPrediction,
    EditPredictionKeybindAction, EditPredictionKeybindSurface, MenuEditPredictionsPolicy,
    MultiBuffer, ShowCompletions,
    editor_tests::{init_test, update_test_language_settings},
    test::{
        build_editor, editor_lsp_test_context::EditorLspTestContext,
        editor_test_context::EditorTestContext,
    },
};
use rpc::proto::PeerId;
use workspace::CollaboratorId;

mod basic_edits;
mod cursor_popover_keybindings;
mod dismissal;
mod inline_keybindings;
mod occlusion;
mod preview;
mod support;

pub use support::FakeEditPredictionDelegate;
