use super::*;

impl Global for ThreadMetadataStore {}

#[derive(Clone, Debug)]
pub enum ThreadMetadataStoreEvent {
    ThreadArchived(ThreadId),
}

impl gpui::EventEmitter<ThreadMetadataStoreEvent> for ThreadMetadataStore {}
