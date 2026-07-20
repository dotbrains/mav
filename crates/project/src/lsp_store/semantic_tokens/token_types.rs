use std::{ops::Range, sync::Arc};

use collections::HashMap;
use language::LanguageServerId;
use text::Anchor;

#[derive(Debug, Default, Clone)]
pub struct BufferSemanticTokens {
    pub tokens: Option<HashMap<LanguageServerId, Arc<[BufferSemanticToken]>>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TokenType(pub u32);

#[derive(Debug, Clone)]
pub struct BufferSemanticToken {
    /// The range of the token in the buffer.
    ///
    /// Guaranteed to contain a buffer id.
    pub range: Range<Anchor>,
    pub token_type: TokenType,
    pub token_modifiers: u32,
}
