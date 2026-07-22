use super::*;

impl CollabPanel {
    fn render_filter_input(
        &self,
        editor: &Entity<Editor>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let settings = ThemeSettings::get_global(cx);
        let text_style = TextStyle {
            color: if editor.read(cx).read_only(cx) {
                cx.theme().colors().text_disabled
            } else {
                cx.theme().colors().text
            },
            font_family: settings.ui_font.family.clone(),
            font_features: settings.ui_font.features.clone(),
            font_fallbacks: settings.ui_font.fallbacks.clone(),
            font_size: rems(0.875).into(),
            font_weight: settings.ui_font.weight,
            font_style: FontStyle::Normal,
            line_height: relative(1.3),
            ..Default::default()
        };

        EditorElement::new(
            editor,
            EditorStyle {
                local_player: cx.theme().players().local(),
                text: text_style,
                ..Default::default()
            },
        )
    }

    fn render_header(
        &self,
        section: Section,
        is_selected: bool,
        is_collapsed: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let mut channel_link = None;
        let mut channel_tooltip_text = None;
        let mut channel_icon = None;

        let text = match section {
            Section::ActiveCall => {
                let channel_name = maybe!({
                    let channel_id = ActiveCall::global(cx).read(cx).channel_id(cx)?;

                    let channel = self.channel_store.read(cx).channel_for_id(channel_id)?;

                    channel_link = Some(channel.link(cx));
                    (channel_icon, channel_tooltip_text) = match channel.visibility {
                        proto::ChannelVisibility::Public => {
                            (Some("icons/public.svg"), Some("Copy public channel link."))
                        }
                        proto::ChannelVisibility::Members => {
                            (Some("icons/hash.svg"), Some("Copy private channel link."))
                        }
                    };

                    Some(channel.name.clone())
                });

                if let Some(name) = channel_name {
                    name
                } else {
                    SharedString::from("Current Call")
                }
            }
            Section::FavoriteChannels => SharedString::from("Favorites"),
            Section::ContactRequests => SharedString::from("Requests"),
            Section::Contacts => SharedString::from("Contacts"),
            Section::Channels => SharedString::from("Channels"),
            Section::ChannelInvites => SharedString::from("Invites"),
            Section::Online => SharedString::from("Online"),
            Section::Offline => SharedString::from("Offline"),
        };

        let auto_watch_state = self
            .workspace
            .upgrade()
            .map_or(AutoWatch::Off, |workspace| {
                *workspace.read(cx).auto_watch_state()
            });
        let is_auto_watching = auto_watch_state.enabled();

        let button = match section {
            Section::ActiveCall => Some(
                h_flex()
                    .when_some(channel_link, |this, channel_link| {
                        this.child(
                            CopyButton::new("copy-channel-link", channel_link)
                                .visible_on_hover("section-header")
                                .tooltip_label("Copy Channel Link"),
                        )
                    })
                    .child(
                        IconButton::new(
                            "auto-watch-screens",
                            if is_auto_watching {
                                IconName::Eye
                            } else {
                                IconName::EyeOff
                            },
                        )
                        .icon_size(IconSize::Small)
                        .toggle_state(is_auto_watching)
                        .selected_style(match auto_watch_state {
                            AutoWatch::Paused => ButtonStyle::Tinted(TintColor::Warning),
                            _ => ButtonStyle::Tinted(TintColor::Accent),
                        })
                        .when(!is_auto_watching, |this| {
                            this.visible_on_hover("section-header")
                        })
                        .tooltip(Tooltip::text(match auto_watch_state {
                            AutoWatch::Paused => "Auto Watch Screens (paused while sharing)",
                            AutoWatch::Active { .. } => "Stop Auto Watching Screens",
                            AutoWatch::Off => "Auto Watch Screens",
                        }))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.workspace
                                .update(cx, |workspace, cx| workspace.toggle_auto_watch(window, cx))
                                .ok();
                        })),
                    )
                    .into_any_element(),
            ),
            Section::Contacts => Some(
                IconButton::new("add-contact", IconName::Plus)
                    .icon_size(IconSize::Small)
                    .on_click(
                        cx.listener(|this, _, window, cx| this.toggle_contact_finder(window, cx)),
                    )
                    .tooltip(Tooltip::text("Search for new contact"))
                    .into_any_element(),
            ),
            Section::Channels => {
                Some(
                    h_flex()
                        .child(
                            IconButton::new("filter-occupied-channels", IconName::ListFilter)
                                .icon_size(IconSize::Small)
                                .toggle_state(self.filter_occupied_channels)
                                .on_click(cx.listener(|this, _, _window, cx| {
                                    this.filter_occupied_channels = !this.filter_occupied_channels;
                                    this.update_entries(true, cx);
                                    this.persist_filter_occupied_channels(cx);
                                }))
                                .tooltip(Tooltip::text(if self.filter_occupied_channels {
                                    "Show All Channels"
                                } else {
                                    "Show Occupied Channels"
                                })),
                        )
                        .child(
                            IconButton::new("add-channel", IconName::Plus)
                                .icon_size(IconSize::Small)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.new_root_channel(window, cx)
                                }))
                                .tooltip(Tooltip::text("Create Channel")),
                        )
                        .into_any_element(),
                )
            }
            _ => None,
        };

        let can_collapse = match section {
            Section::ActiveCall
            | Section::Channels
            | Section::Contacts
            | Section::FavoriteChannels => false,

            Section::ChannelInvites
            | Section::ContactRequests
            | Section::Online
            | Section::Offline => true,
        };

        h_flex().w_full().group("section-header").child(
            ListHeader::new(text)
                .when(can_collapse, |header| {
                    header.toggle(Some(!is_collapsed)).on_toggle(cx.listener(
                        move |this, _, _, cx| {
                            this.toggle_section_expanded(section, cx);
                        },
                    ))
                })
                .inset(true)
                .end_slot::<AnyElement>(button)
                .toggle_state(is_selected),
        )
    }
}
