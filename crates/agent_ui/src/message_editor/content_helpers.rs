use super::*;

/// Walks the editor's creases in order, interleaving plain-text chunks from
/// `text` with mention blocks produced from `resolve`.
fn build_chunks_from_creases(
    text: &str,
    crease_snapshot: &CreaseSnapshot,
    buffer_snapshot: &MultiBufferSnapshot,
    supports_embedded_context: bool,
    mut resolve: impl FnMut(&CreaseId) -> Option<(MentionUri, Option<Mention>)>,
) -> (Vec<acp::ContentBlock>, Vec<Entity<Buffer>>) {
    let mut ix = text
        .char_indices()
        .find(|(_, c)| !c.is_whitespace())
        .map_or(text.len(), |(i, _)| i);
    let mut chunks = Vec::new();
    let mut tracked_buffers = Vec::new();

    for (crease_id, crease) in crease_snapshot.creases() {
        let Some((uri, mention)) = resolve(&crease_id) else {
            continue;
        };
        let crease_range = crease.range().to_offset(buffer_snapshot);
        if crease_range.start.0 > ix {
            chunks.push(text[ix..crease_range.start.0].into());
        }
        chunks.push(mention_to_content_block(
            &uri,
            mention.as_ref(),
            supports_embedded_context,
            &mut tracked_buffers,
        ));
        ix = crease_range.end.0;
    }

    if ix < text.len() {
        let last_chunk = text[ix..].trim_end().to_owned();
        if !last_chunk.is_empty() {
            chunks.push(last_chunk.into());
        }
    }
    (chunks, tracked_buffers)
}

fn image_preview_task_for_mention(
    mention: &Mention,
) -> Option<futures::future::Shared<Task<Result<Arc<Image>, String>>>> {
    let Mention::Image(mention_image) = mention else {
        return None;
    };

    let bytes =
        match base64::engine::general_purpose::STANDARD.decode(mention_image.data.as_bytes()) {
            Ok(bytes) => bytes,
            Err(error) => {
                log::error!("failed to decode image mention: {error}");
                return None;
            }
        };

    Some(
        Task::ready(Ok::<Arc<Image>, String>(Arc::new(Image::from_bytes(
            mention_image.format,
            bytes,
        ))))
        .shared(),
    )
}

fn mention_to_content_block(
    uri: &MentionUri,
    mention: Option<&Mention>,
    supports_embedded_context: bool,
    tracked_buffers: &mut Vec<Entity<Buffer>>,
) -> acp::ContentBlock {
    match mention {
        Some(Mention::Text {
            content,
            tracked_buffers: mention_tracked_buffers,
        }) => {
            tracked_buffers.extend(mention_tracked_buffers.iter().cloned());
            if supports_embedded_context {
                acp::ContentBlock::Resource(acp::EmbeddedResource::new(
                    acp::EmbeddedResourceResource::TextResourceContents(
                        acp::TextResourceContents::new(content.clone(), uri.to_uri().to_string()),
                    ),
                ))
            } else {
                acp::ContentBlock::ResourceLink(acp::ResourceLink::new(
                    uri.name(),
                    uri.to_uri().to_string(),
                ))
            }
        }
        Some(Mention::Image(mention_image)) => acp::ContentBlock::Image(
            acp::ImageContent::new(mention_image.data.clone(), mention_image.format.mime_type())
                .uri(match uri {
                    MentionUri::File { .. } | MentionUri::PastedImage { .. } => {
                        Some(uri.to_uri().to_string())
                    }
                    other => {
                        debug_panic!("unexpected mention uri for image: {:?}", other);
                        None
                    }
                }),
        ),
        _ => acp::ContentBlock::ResourceLink(acp::ResourceLink::new(
            uri.name(),
            uri.to_uri().to_string(),
        )),
    }
}

/// Parses markdown mention links in the format `[@name](uri)` from text.
/// Returns a vector of (range, MentionUri) pairs where range is the byte range in the text.
fn parse_mention_links(text: &str, path_style: PathStyle) -> Vec<(Range<usize>, MentionUri)> {
    let mut mentions = Vec::new();
    let mut search_start = 0;

    while let Some(link_start) = text[search_start..].find("[@") {
        let absolute_start = search_start + link_start;

        // Find the matching closing bracket for the name, handling nested brackets.
        // Start at the '[' character so find_matching_bracket can track depth correctly.
        let Some(name_end) = find_matching_bracket(&text[absolute_start..], '[', ']') else {
            search_start = absolute_start + 2;
            continue;
        };
        let name_end = absolute_start + name_end;

        // Check for opening parenthesis immediately after
        if text.get(name_end + 1..name_end + 2) != Some("(") {
            search_start = name_end + 1;
            continue;
        }

        // Find the matching closing parenthesis for the URI, handling nested parens
        let uri_start = name_end + 2;
        let Some(uri_end_relative) = find_matching_bracket(&text[name_end + 1..], '(', ')') else {
            search_start = uri_start;
            continue;
        };
        let uri_end = name_end + 1 + uri_end_relative;
        let link_end = uri_end + 1;

        let uri_str = &text[uri_start..uri_end];

        // Try to parse the URI as a MentionUri
        if let Ok(mention_uri) = MentionUri::parse(uri_str, path_style) {
            mentions.push((absolute_start..link_end, mention_uri));
        }

        search_start = link_end;
    }

    mentions
}

/// Finds the position of the matching closing bracket, handling nested brackets.
/// The input `text` should start with the opening bracket.
/// Returns the index of the matching closing bracket relative to `text`.
fn find_matching_bracket(text: &str, open: char, close: char) -> Option<usize> {
    let mut depth = 0;
    for (index, character) in text.char_indices() {
        if character == open {
            depth += 1;
        } else if character == close {
            depth -= 1;
            if depth == 0 {
                return Some(index);
            }
        }
    }
    None
}
