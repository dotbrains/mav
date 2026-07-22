use super::super::*;
use super::context::{RUST_BIN_KIND_TASK_VARIABLE, RUST_BIN_NAME_TASK_VARIABLE, test_fragment};
use super::metadata::{
    CargoMetadata, TargetInfo, TargetKind, package_name_from_pkgid, target_info_from_abs_path,
    target_info_from_metadata,
};
use crate::language;
use gpui::{BorrowAppContext, Hsla, TestAppContext};
use lsp::CompletionItemLabelDetails;
use settings::SettingsStore;
use std::num::NonZeroU32;
use theme::SyntaxTheme;
use util::path;

mod completion;
mod diagnostics;
mod metadata;
mod symbol_indent;
