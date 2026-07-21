use super::*;

pub(super) fn language_server_for_buffer(
    lsp_store: &Entity<LspStore>,
    buffer: &Entity<Buffer>,
    server_id: LanguageServerId,
    cx: &mut AsyncApp,
) -> Result<(Arc<CachedLspAdapter>, Arc<LanguageServer>)> {
    lsp_store
        .update(cx, |lsp_store, cx| {
            buffer.update(cx, |buffer, cx| {
                lsp_store
                    .language_server_for_local_buffer(buffer, server_id, cx)
                    .map(|(adapter, server)| (adapter.clone(), server.clone()))
            })
        })
        .context("no language server found for buffer")
}

pub async fn location_links_from_proto(
    proto_links: Vec<proto::LocationLink>,
    lsp_store: Entity<LspStore>,
    mut cx: AsyncApp,
) -> Result<Vec<LocationLink>> {
    let mut links = Vec::new();

    for link in proto_links {
        links.push(location_link_from_proto(link, lsp_store.clone(), &mut cx).await?)
    }

    Ok(links)
}

pub fn location_link_from_proto(
    link: proto::LocationLink,
    lsp_store: Entity<LspStore>,
    cx: &mut AsyncApp,
) -> Task<Result<LocationLink>> {
    cx.spawn(async move |cx| {
        let origin = match link.origin {
            Some(origin) => {
                let buffer_id = BufferId::new(origin.buffer_id)?;
                let buffer = lsp_store
                    .update(cx, |lsp_store, cx| {
                        lsp_store.wait_for_remote_buffer(buffer_id, cx)
                    })
                    .await?;
                let start = origin
                    .start
                    .and_then(deserialize_anchor)
                    .context("missing origin start")?;
                let end = origin
                    .end
                    .and_then(deserialize_anchor)
                    .context("missing origin end")?;
                buffer
                    .update(cx, |buffer, _| buffer.wait_for_anchors([start, end]))
                    .await?;
                Some(Location {
                    buffer,
                    range: start..end,
                })
            }
            None => None,
        };

        let target = link.target.context("missing target")?;
        let buffer_id = BufferId::new(target.buffer_id)?;
        let buffer = lsp_store
            .update(cx, |lsp_store, cx| {
                lsp_store.wait_for_remote_buffer(buffer_id, cx)
            })
            .await?;
        let start = target
            .start
            .and_then(deserialize_anchor)
            .context("missing target start")?;
        let end = target
            .end
            .and_then(deserialize_anchor)
            .context("missing target end")?;
        buffer
            .update(cx, |buffer, _| buffer.wait_for_anchors([start, end]))
            .await?;
        let target = Location {
            buffer,
            range: start..end,
        };
        Ok(LocationLink { origin, target })
    })
}

pub async fn location_links_from_lsp(
    message: Option<lsp::GotoDefinitionResponse>,
    lsp_store: Entity<LspStore>,
    buffer: Entity<Buffer>,
    server_id: LanguageServerId,
    workspace_only: bool,
    mut cx: AsyncApp,
) -> Result<Vec<LocationLink>> {
    let message = match message {
        Some(message) => message,
        None => return Ok(Vec::new()),
    };

    let mut unresolved_links = Vec::new();
    match message {
        lsp::GotoDefinitionResponse::Scalar(loc) => {
            unresolved_links.push((None, loc.uri, loc.range));
        }

        lsp::GotoDefinitionResponse::Array(locs) => {
            unresolved_links.extend(locs.into_iter().map(|l| (None, l.uri, l.range)));
        }

        lsp::GotoDefinitionResponse::Link(links) => {
            unresolved_links.extend(links.into_iter().map(|l| {
                (
                    l.origin_selection_range,
                    l.target_uri,
                    l.target_selection_range,
                )
            }));
        }
    }

    let (_, language_server) = language_server_for_buffer(&lsp_store, &buffer, server_id, &mut cx)?;
    let mut definitions = Vec::new();
    for (origin_range, target_uri, target_range) in unresolved_links {
        if workspace_only
            && !lsp_store.update(&mut cx, |this, cx| {
                use util::paths::UrlExt as _;
                let worktree_store = this.worktree_store().read(cx);
                let path_style = worktree_store.path_style();
                let Ok(abs_path) = target_uri.clone().to_file_path_ext(path_style) else {
                    return false;
                };
                worktree_store
                    .find_worktree(&abs_path, cx)
                    .is_some_and(|(worktree, _)| {
                        let worktree = worktree.read(cx);
                        worktree.is_visible() && !worktree.is_single_file()
                    })
            })
        {
            continue;
        }

        let target_buffer_handle = lsp_store
            .update(&mut cx, |this, cx| {
                this.open_local_buffer_via_lsp(target_uri, language_server.server_id(), cx)
            })
            .await?;

        cx.update(|cx| {
            let origin_location = origin_range.map(|origin_range| {
                let origin_buffer = buffer.read(cx);
                let origin_start =
                    origin_buffer.clip_point_utf16(point_from_lsp(origin_range.start), Bias::Left);
                let origin_end =
                    origin_buffer.clip_point_utf16(point_from_lsp(origin_range.end), Bias::Left);
                Location {
                    buffer: buffer.clone(),
                    range: origin_buffer.anchor_after(origin_start)
                        ..origin_buffer.anchor_before(origin_end),
                }
            });

            let target_buffer = target_buffer_handle.read(cx);
            let target_start =
                target_buffer.clip_point_utf16(point_from_lsp(target_range.start), Bias::Left);
            let target_end =
                target_buffer.clip_point_utf16(point_from_lsp(target_range.end), Bias::Left);
            let target_location = Location {
                buffer: target_buffer_handle,
                range: target_buffer.anchor_after(target_start)
                    ..target_buffer.anchor_before(target_end),
            };

            definitions.push(LocationLink {
                origin: origin_location,
                target: target_location,
            })
        });
    }
    Ok(definitions)
}

pub async fn location_link_from_lsp(
    link: lsp::LocationLink,
    lsp_store: &Entity<LspStore>,
    buffer: &Entity<Buffer>,
    server_id: LanguageServerId,
    cx: &mut AsyncApp,
) -> Result<LocationLink> {
    let (_, language_server) = language_server_for_buffer(lsp_store, buffer, server_id, cx)?;

    let (origin_range, target_uri, target_range) = (
        link.origin_selection_range,
        link.target_uri,
        link.target_selection_range,
    );

    let target_buffer_handle = lsp_store
        .update(cx, |lsp_store, cx| {
            lsp_store.open_local_buffer_via_lsp(target_uri, language_server.server_id(), cx)
        })
        .await?;

    Ok(cx.update(|cx| {
        let origin_location = origin_range.map(|origin_range| {
            let origin_buffer = buffer.read(cx);
            let origin_start =
                origin_buffer.clip_point_utf16(point_from_lsp(origin_range.start), Bias::Left);
            let origin_end =
                origin_buffer.clip_point_utf16(point_from_lsp(origin_range.end), Bias::Left);
            Location {
                buffer: buffer.clone(),
                range: origin_buffer.anchor_after(origin_start)
                    ..origin_buffer.anchor_before(origin_end),
            }
        });

        let target_buffer = target_buffer_handle.read(cx);
        let target_start =
            target_buffer.clip_point_utf16(point_from_lsp(target_range.start), Bias::Left);
        let target_end =
            target_buffer.clip_point_utf16(point_from_lsp(target_range.end), Bias::Left);
        let target_location = Location {
            buffer: target_buffer_handle,
            range: target_buffer.anchor_after(target_start)
                ..target_buffer.anchor_before(target_end),
        };

        LocationLink {
            origin: origin_location,
            target: target_location,
        }
    }))
}

pub fn location_links_to_proto(
    links: Vec<LocationLink>,
    lsp_store: &mut LspStore,
    peer_id: PeerId,
    cx: &mut App,
) -> Vec<proto::LocationLink> {
    links
        .into_iter()
        .map(|definition| location_link_to_proto(definition, lsp_store, peer_id, cx))
        .collect()
}

pub fn location_link_to_proto(
    location: LocationLink,
    lsp_store: &mut LspStore,
    peer_id: PeerId,
    cx: &mut App,
) -> proto::LocationLink {
    let origin = location.origin.map(|origin| {
        lsp_store
            .buffer_store()
            .update(cx, |buffer_store, cx| {
                buffer_store.create_buffer_for_peer(&origin.buffer, peer_id, cx)
            })
            .detach_and_log_err(cx);

        let buffer_id = origin.buffer.read(cx).remote_id().into();
        proto::Location {
            start: Some(serialize_anchor(&origin.range.start)),
            end: Some(serialize_anchor(&origin.range.end)),
            buffer_id,
        }
    });

    lsp_store
        .buffer_store()
        .update(cx, |buffer_store, cx| {
            buffer_store.create_buffer_for_peer(&location.target.buffer, peer_id, cx)
        })
        .detach_and_log_err(cx);

    let buffer_id = location.target.buffer.read(cx).remote_id().into();
    let target = proto::Location {
        start: Some(serialize_anchor(&location.target.range.start)),
        end: Some(serialize_anchor(&location.target.range.end)),
        buffer_id,
    };

    proto::LocationLink {
        origin,
        target: Some(target),
    }
}
