use std::sync::Arc;

use collections::HashMap;
use language::LanguageServerId;
use smol::future::yield_now;
use text::{Bias, OffsetUtf16, PointUtf16, Unclipped};

use super::{BufferSemanticToken, RawSemanticTokens};

pub(super) async fn raw_to_buffer_semantic_tokens(
    raw_tokens: RawSemanticTokens,
    buffer_snapshot: text::BufferSnapshot,
) -> HashMap<LanguageServerId, Arc<[BufferSemanticToken]>> {
    let mut res = HashMap::default();
    for (&server_id, server_tokens) in &raw_tokens.servers {
        let mut last = 0;
        let mut buffer_tokens = Vec::with_capacity(server_tokens.data.len() / 5);
        let mut tokens = server_tokens.tokens();
        const CHUNK_LEN: usize = 5000;

        loop {
            let mut changed = false;
            let chunk = tokens
                .by_ref()
                .take(CHUNK_LEN)
                .inspect(|_| changed = true)
                .filter_map(|token| {
                    let start = Unclipped(PointUtf16::new(token.line, token.start));
                    let clipped_start = buffer_snapshot.clip_point_utf16(start, Bias::Left);
                    let start_offset = buffer_snapshot
                        .as_rope()
                        .point_utf16_to_offset_utf16(clipped_start);
                    let end_offset = start_offset + OffsetUtf16(token.length as usize);

                    let start = buffer_snapshot
                        .as_rope()
                        .offset_utf16_to_offset(start_offset);
                    if start < last {
                        return None;
                    }

                    let end = buffer_snapshot.as_rope().offset_utf16_to_offset(end_offset);
                    last = end;

                    if start == end {
                        return None;
                    }

                    Some(BufferSemanticToken {
                        range: buffer_snapshot.anchor_range_inside(start..end),
                        token_type: token.token_type,
                        token_modifiers: token.token_modifiers,
                    })
                });
            buffer_tokens.extend(chunk);

            if !changed {
                break;
            }
            yield_now().await;
        }

        res.insert(server_id, buffer_tokens.into());
    }
    res
}
