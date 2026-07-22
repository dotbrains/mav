use super::*;

impl ExtensionsPage {
    pub(super) fn render_extensions(
        &mut self,
        range: Range<usize>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<ExtensionCard> {
        let dev_extension_entries_len = if self.filter.include_dev_extensions() {
            self.filtered_dev_extension_indices.len()
        } else {
            0
        };
        range
            .map(|ix| {
                if ix < dev_extension_entries_len {
                    let dev_ix = self.filtered_dev_extension_indices[ix];
                    let extension = &self.dev_extension_entries[dev_ix];
                    self.render_dev_extension(extension, cx)
                } else {
                    let extension_ix =
                        self.filtered_remote_extension_indices[ix - dev_extension_entries_len];
                    let extension = &self.remote_extension_entries[extension_ix];
                    self.render_remote_extension(extension, cx)
                }
            })
            .collect()
    }

    pub(super) fn render_dev_extension(
        &self,
        extension: &ExtensionManifest,
        cx: &mut Context<Self>,
    ) -> ExtensionCard {
        let status = Self::extension_status(&extension.id, cx);

        let repository_url = extension.repository.clone();

        let can_configure = !extension.context_servers.is_empty();

        ExtensionCard::new()
            .child(
                h_flex()
                    .justify_between()
                    .child(
                        h_flex()
                            .gap_2()
                            .items_end()
                            .child(Headline::new(extension.name.clone()).size(HeadlineSize::Medium))
                            .child(
                                Headline::new(format!("v{}", extension.version))
                                    .size(HeadlineSize::XSmall),
                            ),
                    )
                    .child(
                        h_flex()
                            .gap_1()
                            .justify_between()
                            .child(
                                Button::new(
                                    SharedString::from(format!("rebuild-{}", extension.id)),
                                    "Rebuild",
                                )
                                .color(Color::Accent)
                                .disabled(matches!(status, ExtensionStatus::Upgrading))
                                .on_click({
                                    let extension_id = extension.id.clone();
                                    move |_, _, cx| {
                                        ExtensionStore::global(cx).update(cx, |store, cx| {
                                            store.rebuild_dev_extension(extension_id.clone(), cx)
                                        });
                                    }
                                }),
                            )
                            .child(
                                Button::new(extension_button_id(&extension.id, ExtensionOperation::Remove), "Uninstall")
                                    .color(Color::Accent)
                                    .disabled(matches!(status, ExtensionStatus::Removing))
                                    .on_click({
                                        let extension_id = extension.id.clone();
                                        move |_, _, cx| {
                                            ExtensionStore::global(cx).update(cx, |store, cx| {
                                                store.uninstall_extension(extension_id.clone(), cx).detach_and_log_err(cx);
                                            });
                                        }
                                    }),
                            )
                            .when(can_configure, |this| {
                                this.child(
                                    Button::new(
                                        SharedString::from(format!("configure-{}", extension.id)),
                                        "Configure",
                                    )
                                    .color(Color::Accent)
                                    .disabled(matches!(status, ExtensionStatus::Installing))
                                    .on_click({
                                        let manifest = Arc::new(extension.clone());
                                        move |_, _, cx| {
                                            if let Some(events) =
                                                extension::ExtensionEvents::try_global(cx)
                                            {
                                                events.update(cx, |this, cx| {
                                                    this.emit(
                                                        extension::Event::ConfigureExtensionRequested(
                                                            manifest.clone(),
                                                        ),
                                                        cx,
                                                    )
                                                });
                                            }
                                        }
                                    }),
                                )
                            }),
                    ),
            )
            .child(
                h_flex()
                    .gap_2()
                    .justify_between()
                    .child(
                        Label::new(format!(
                            "{}: {}",
                            if extension.authors.len() > 1 {
                                "Authors"
                            } else {
                                "Author"
                            },
                            extension.authors.join(", ")
                        ))
                        .size(LabelSize::Small)
                        .color(Color::Muted)
                        .truncate(),
                    )
                    .child(Label::new("<>").size(LabelSize::Small)),
            )
            .child(
                h_flex()
                    .gap_2()
                    .justify_between()
                    .children(extension.description.as_ref().map(|description| {
                        Label::new(description.clone())
                            .size(LabelSize::Small)
                            .color(Color::Default)
                            .truncate()
                    }))
                    .children(repository_url.map(|repository_url| {
                        IconButton::new(
                            SharedString::from(format!("repository-{}", extension.id)),
                            IconName::Github,
                        )
                        .icon_color(Color::Accent)
                        .icon_size(IconSize::Small)
                        .on_click(cx.listener({
                            let repository_url = repository_url.clone();
                            move |_, _, _, cx| {
                                cx.open_url(&repository_url);
                            }
                        }))
                        .tooltip(Tooltip::text(repository_url))
                    })),
            )
    }

    pub(super) fn render_remote_extension(
        &self,
        extension: &ExtensionMetadata,
        cx: &mut Context<Self>,
    ) -> ExtensionCard {
        let this = cx.weak_entity();
        let status = Self::extension_status(&extension.id, cx);
        let has_dev_extension = Self::dev_extension_exists(&extension.id, cx);

        let extension_id = extension.id.clone();
        let buttons = self.buttons_for_entry(extension, &status, has_dev_extension, cx);
        let version = extension.manifest.version.clone();
        let repository_url = extension.manifest.repository.clone();
        let authors = extension.manifest.authors.clone();

        let installed_version = match status {
            ExtensionStatus::Installed(installed_version) => Some(installed_version),
            _ => None,
        };

        ExtensionCard::new()
            .overridden_by_dev_extension(has_dev_extension)
            .child(
                h_flex()
                    .justify_between()
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                Headline::new(extension.manifest.name.clone())
                                    .size(HeadlineSize::Small),
                            )
                            .child(Headline::new(format!("v{version}")).size(HeadlineSize::XSmall))
                            .children(
                                installed_version
                                    .filter(|installed_version| *installed_version != version)
                                    .map(|installed_version| {
                                        Headline::new(format!("(v{installed_version} installed)",))
                                            .size(HeadlineSize::XSmall)
                                    }),
                            )
                            .map(|parent| {
                                if extension.manifest.provides.is_empty() {
                                    return parent;
                                }

                                parent.child(
                                    h_flex().gap_1().children(
                                        extension
                                            .manifest
                                            .provides
                                            .iter()
                                            .filter_map(|provides| {
                                                match provides {
                                                    ExtensionProvides::AgentServers
                                                    | ExtensionProvides::SlashCommands
                                                    | ExtensionProvides::IndexedDocsProviders => {
                                                        return None;
                                                    }
                                                    _ => {}
                                                }

                                                Some(Chip::new(extension_provides_label(*provides)))
                                            })
                                            .collect::<Vec<_>>(),
                                    ),
                                )
                            }),
                    )
                    .child(
                        h_flex()
                            .gap_1()
                            .children(buttons.upgrade)
                            .children(buttons.configure)
                            .child(buttons.install_or_uninstall),
                    ),
            )
            .child(
                h_flex()
                    .gap_2()
                    .justify_between()
                    .children(extension.manifest.description.as_ref().map(|description| {
                        Label::new(description.clone())
                            .size(LabelSize::Small)
                            .color(Color::Default)
                            .truncate()
                    }))
                    .child(
                        Label::new(format!(
                            "Downloads: {}",
                            extension.download_count.to_formatted_string(&Locale::en)
                        ))
                        .size(LabelSize::Small),
                    ),
            )
            .child(
                h_flex()
                    .min_w_0()
                    .w_full()
                    .justify_between()
                    .child(
                        h_flex()
                            .min_w_0()
                            .gap_1()
                            .child(
                                Icon::new(IconName::Person)
                                    .size(IconSize::XSmall)
                                    .color(Color::Muted),
                            )
                            .child(
                                Label::new(extension.manifest.authors.join(", "))
                                    .size(LabelSize::Small)
                                    .color(Color::Muted)
                                    .truncate(),
                            ),
                    )
                    .child(
                        h_flex()
                            .gap_1()
                            .flex_shrink_0()
                            .child({
                                let repo_url_for_tooltip = repository_url.clone();

                                IconButton::new(
                                    SharedString::from(format!("repository-{}", extension.id)),
                                    IconName::Github,
                                )
                                .icon_size(IconSize::Small)
                                .tooltip(move |_, cx| {
                                    Tooltip::with_meta(
                                        "Visit Extension Repository",
                                        None,
                                        repo_url_for_tooltip.clone(),
                                        cx,
                                    )
                                })
                                .on_click(cx.listener(
                                    move |_, _, _, cx| {
                                        cx.open_url(&repository_url);
                                    },
                                ))
                            })
                            .child(
                                PopoverMenu::new(SharedString::from(format!(
                                    "more-{}",
                                    extension.id
                                )))
                                .trigger(
                                    IconButton::new(
                                        SharedString::from(format!("more-{}", extension.id)),
                                        IconName::Ellipsis,
                                    )
                                    .icon_size(IconSize::Small),
                                )
                                .anchor(Anchor::TopRight)
                                .offset(Point {
                                    x: px(0.0),
                                    y: px(2.0),
                                })
                                .menu(move |window, cx| {
                                    this.upgrade().map(|this| {
                                        Self::render_remote_extension_context_menu(
                                            &this,
                                            extension_id.clone(),
                                            authors.clone(),
                                            window,
                                            cx,
                                        )
                                    })
                                }),
                            ),
                    ),
            )
    }

    pub(super) fn render_remote_extension_context_menu(
        this: &Entity<Self>,
        extension_id: Arc<str>,
        authors: Vec<String>,
        window: &mut Window,
        cx: &mut App,
    ) -> Entity<ContextMenu> {
        ContextMenu::build(window, cx, |context_menu, window, _| {
            context_menu
                .entry(
                    "Install Another Version...",
                    None,
                    window.handler_for(this, {
                        let extension_id = extension_id.clone();
                        move |this, window, cx| {
                            this.show_extension_version_list(extension_id.clone(), window, cx)
                        }
                    }),
                )
                .entry("Copy Extension ID", None, {
                    let extension_id = extension_id.clone();
                    move |_, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(extension_id.to_string()));
                    }
                })
                .entry("Copy Author Info", None, {
                    let authors = authors.clone();
                    move |_, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(authors.join(", ")));
                    }
                })
        })
    }

    pub(super) fn show_extension_version_list(
        &mut self,
        extension_id: Arc<str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace) = self.workspace.upgrade() else {
            return;
        };

        cx.spawn_in(window, async move |this, cx| {
            let extension_versions_task = this.update(cx, |_, cx| {
                let extension_store = ExtensionStore::global(cx);

                extension_store.update(cx, |store, cx| {
                    store.fetch_extension_versions(&extension_id, cx)
                })
            })?;

            let extension_versions = extension_versions_task.await?;

            workspace.update_in(cx, |workspace, window, cx| {
                let fs = workspace.project().read(cx).fs().clone();
                workspace.toggle_modal(window, cx, |window, cx| {
                    let delegate = ExtensionVersionSelectorDelegate::new(
                        fs,
                        cx.entity().downgrade(),
                        extension_versions,
                    );

                    ExtensionVersionSelector::new(delegate, window, cx)
                });
            })?;

            anyhow::Ok(())
        })
        .detach_and_log_err(cx);
    }
}
