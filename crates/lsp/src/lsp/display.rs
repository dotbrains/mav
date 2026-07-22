use super::*;

impl Drop for LanguageServer {
    fn drop(&mut self) {
        if let Some(shutdown) = self.shutdown() {
            self.executor.spawn(shutdown).detach();
        }
    }
}

impl Subscription {
    /// Detaching a subscription handle prevents it from unsubscribing on drop.
    pub fn detach(&mut self) {
        match self {
            Subscription::Notification {
                notification_handlers,
                ..
            } => *notification_handlers = None,
            Subscription::Io { io_handlers, .. } => *io_handlers = None,
        }
    }
}

impl fmt::Display for LanguageServerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl fmt::Debug for LanguageServer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LanguageServer")
            .field("id", &self.server_id.0)
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

impl fmt::Debug for LanguageServerBinary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut debug = f.debug_struct("LanguageServerBinary");
        debug.field("path", &self.path);
        debug.field("arguments", &self.arguments);

        if let Some(env) = &self.env {
            let redacted_env: BTreeMap<String, String> = env
                .iter()
                .map(|(key, value)| {
                    let redacted_value = if redact::should_redact(key) {
                        "REDACTED".to_string()
                    } else {
                        value.clone()
                    };
                    (key.clone(), redacted_value)
                })
                .collect();
            debug.field("env", &Some(redacted_env));
        } else {
            debug.field("env", &self.env);
        }

        debug.finish()
    }
}

impl Drop for Subscription {
    fn drop(&mut self) {
        match self {
            Subscription::Notification {
                method,
                notification_handlers,
            } => {
                if let Some(handlers) = notification_handlers.as_ref().and_then(|h| h.upgrade()) {
                    handlers.lock().remove(method);
                }
            }
            Subscription::Io { id, io_handlers } => {
                if let Some(io_handlers) = io_handlers.as_ref().and_then(|h| h.upgrade()) {
                    io_handlers.lock().remove(id);
                }
            }
        }
    }
}
