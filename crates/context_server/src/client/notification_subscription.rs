use gpui::AsyncApp;
use parking_lot::Mutex;
use serde_json::Value;
use slotmap::SlotMap;
use std::sync::Arc;

use super::NotificationHandler;

slotmap::new_key_type! {
    struct NotificationSubscriptionId;
}

#[derive(Default)]
pub struct NotificationSubscriptionSet {
    // we have very few subscriptions at the moment
    methods: Vec<(&'static str, Vec<NotificationSubscriptionId>)>,
    handlers: SlotMap<NotificationSubscriptionId, NotificationHandler>,
}

impl NotificationSubscriptionSet {
    #[must_use]
    fn add_handler(
        &mut self,
        method: &'static str,
        handler: NotificationHandler,
    ) -> NotificationSubscriptionId {
        let id = self.handlers.insert(handler);
        if let Some((_, handler_ids)) = self
            .methods
            .iter_mut()
            .find(|(probe_method, _)| method == *probe_method)
        {
            debug_assert!(
                handler_ids.len() < 20,
                "Too many MCP handlers for {}. Consider using a different data structure.",
                method
            );

            handler_ids.push(id);
        } else {
            self.methods.push((method, vec![id]));
        };
        id
    }

    pub(super) fn notify(&mut self, method: &str, payload: Value, cx: &mut AsyncApp) {
        let Some((_, handler_ids)) = self
            .methods
            .iter_mut()
            .find(|(probe_method, _)| method == *probe_method)
        else {
            return;
        };

        for handler_id in handler_ids {
            if let Some(handler) = self.handlers.get_mut(*handler_id) {
                handler(payload.clone(), cx.clone());
            }
        }
    }
}

pub struct NotificationSubscription {
    id: NotificationSubscriptionId,
    set: Arc<Mutex<NotificationSubscriptionSet>>,
}

impl NotificationSubscription {
    pub(super) fn new(
        set: Arc<Mutex<NotificationSubscriptionSet>>,
        method: &'static str,
        handler: NotificationHandler,
    ) -> Self {
        let id = set.lock().add_handler(method, handler);
        Self { id, set }
    }
}

impl Drop for NotificationSubscription {
    fn drop(&mut self) {
        let mut set = self.set.lock();
        set.handlers.remove(self.id);
        set.methods.retain_mut(|(_, handler_ids)| {
            handler_ids.retain(|id| *id != self.id);
            !handler_ids.is_empty()
        });
    }
}
