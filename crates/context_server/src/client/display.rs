use std::fmt;

use super::{Client, ContextServerId};

#[derive(Debug)]
pub struct RequestCanceled;

impl std::error::Error for RequestCanceled {}

impl std::fmt::Display for RequestCanceled {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Context server request was canceled")
    }
}

impl fmt::Display for ContextServerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl fmt::Debug for Client {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Context Server Client")
            .field("id", &self.server_id.0)
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}
