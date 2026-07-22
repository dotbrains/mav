use super::*;

pub struct EditedBufferSnapshot {
    pub(super) text: text::EditedBufferSnapshot,
    pub(super) snapshot: BufferSnapshot,
}

impl EditedBufferSnapshot {
    pub fn snapshot(&self) -> &BufferSnapshot {
        &self.snapshot
    }

    pub fn base_version(&self) -> &clock::Global {
        &self.text.base_version
    }
}
