use std::{collections::BTreeMap, ops::Range};

use gpui::Entity;
use language::Buffer;
use text::{Anchor, BufferId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatTrigger {
    Save,
    Manual,
}

pub enum LspFormatTarget {
    Buffers,
    Ranges(BTreeMap<BufferId, Vec<Range<Anchor>>>),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OpenLspBufferHandle(pub(crate) Entity<OpenLspBuffer>);

pub(crate) struct OpenLspBuffer(pub(crate) Entity<Buffer>);

impl FormatTrigger {
    pub(crate) fn from_proto(value: i32) -> FormatTrigger {
        match value {
            0 => FormatTrigger::Save,
            1 => FormatTrigger::Manual,
            _ => FormatTrigger::Save,
        }
    }
}
