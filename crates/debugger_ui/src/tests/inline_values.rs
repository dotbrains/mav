#![expect(clippy::result_large_err)]
use std::{path::Path, sync::Arc};

use dap::{Scope, StackFrame, Variable, requests::Variables};
use editor::{Editor, EditorMode, MultiBuffer};
use gpui::{BackgroundExecutor, TestAppContext, VisualTestContext};
use language::{
    Language, LanguageConfig, LanguageMatcher, rust_lang, tree_sitter_python,
    tree_sitter_typescript,
};
use project::{FakeFs, Project};
use serde_json::json;
use unindent::Unindent as _;
use util::{path, rel_path::rel_path};

use crate::{
    debugger_panel::DebugPanel,
    tests::{active_debug_session_panel, init_test, init_test_workspace, start_debug_session},
};

mod go;
mod javascript;
mod python;
mod rust;
mod rust_cases;
mod support;

use support::*;
