use super::*;

impl CollabPanel {
    fn toggle_section_expanded(&mut self, section: Section, cx: &mut Context<Self>) {
        if let Some(ix) = self.collapsed_sections.iter().position(|s| *s == section) {
            self.collapsed_sections.remove(ix);
        } else {
            self.collapsed_sections.push(section);
        }
        self.update_entries(false, cx);
    }

    fn collapse_selected_channel(
        &mut self,
        _: &CollapseSelectedChannel,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(channel_id) = self.selected_channel().map(|channel| channel.id) else {
            return;
        };

        if self.is_channel_collapsed(channel_id) {
            return;
        }

        self.toggle_channel_collapsed(channel_id, window, cx);
    }

    fn expand_selected_channel(
        &mut self,
        _: &ExpandSelectedChannel,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(id) = self.selected_channel().map(|channel| channel.id) else {
            return;
        };

        if !self.is_channel_collapsed(id) {
            return;
        }

        self.toggle_channel_collapsed(id, window, cx)
    }

    fn toggle_channel_collapsed(
        &mut self,
        channel_id: ChannelId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.collapsed_channels.binary_search(&channel_id) {
            Ok(ix) => {
                self.collapsed_channels.remove(ix);
            }
            Err(ix) => {
                self.collapsed_channels.insert(ix, channel_id);
            }
        };
        self.serialize(cx);
        self.update_entries(true, cx);
        cx.notify();
        cx.focus_self(window);
    }

    fn is_channel_collapsed(&self, channel_id: ChannelId) -> bool {
        self.collapsed_channels.binary_search(&channel_id).is_ok()
    }

    pub fn toggle_favorite_channel(&mut self, channel_id: ChannelId, cx: &mut Context<Self>) {
        self.channel_store.update(cx, |store, cx| {
            store.toggle_favorite_channel(channel_id, cx);
        });
        self.persist_favorites(cx);
    }

    fn is_channel_favorited(&self, channel_id: ChannelId, cx: &App) -> bool {
        self.channel_store.read(cx).is_channel_favorited(channel_id)
    }

    fn persist_filter_occupied_channels(&mut self, cx: &mut Context<Self>) {
        let is_enabled = self.filter_occupied_channels;
        let kvp_store = KeyValueStore::global(cx);
        self.pending_filter_serialization = cx.background_spawn(
            async move {
                if is_enabled {
                    kvp_store
                        .write_kvp(FILTER_OCCUPIED_CHANNELS_KEY.to_string(), "1".to_string())
                        .await?;
                } else {
                    kvp_store
                        .delete_kvp(FILTER_OCCUPIED_CHANNELS_KEY.to_string())
                        .await?;
                }
                anyhow::Ok(())
            }
            .log_err(),
        );
    }

    fn persist_favorites(&mut self, cx: &mut Context<Self>) {
        let favorite_ids: Vec<u64> = self
            .channel_store
            .read(cx)
            .favorite_channel_ids()
            .iter()
            .map(|id| id.0)
            .collect();
        let kvp_store = KeyValueStore::global(cx);
        self.pending_favorites_serialization = cx.background_spawn(
            async move {
                let json = serde_json::to_string(&favorite_ids)?;
                kvp_store
                    .write_kvp(FAVORITE_CHANNELS_KEY.to_string(), json)
                    .await?;
                anyhow::Ok(())
            }
            .log_err(),
        );
    }
}
