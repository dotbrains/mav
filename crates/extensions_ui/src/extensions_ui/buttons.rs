use super::*;

pub(super) fn extension_button_id(
    extension_id: &Arc<str>,
    operation: ExtensionOperation,
) -> ElementId {
    (SharedString::from(extension_id.clone()), operation as usize).into()
}

pub(super) struct ExtensionCardButtons {
    pub(super) install_or_uninstall: Button,
    pub(super) upgrade: Option<Button>,
    pub(super) configure: Option<Button>,
}

impl ExtensionsPage {
    pub(super) fn buttons_for_entry(
        &self,
        extension: &ExtensionMetadata,
        status: &ExtensionStatus,
        has_dev_extension: bool,
        cx: &mut Context<Self>,
    ) -> ExtensionCardButtons {
        let is_compatible =
            extension_host::is_version_compatible(ReleaseChannel::global(cx), extension);

        if has_dev_extension {
            // If we have a dev extension for the given extension, just treat it as uninstalled.
            // The button here is a placeholder, as it won't be interactable anyways.
            return ExtensionCardButtons {
                install_or_uninstall: Button::new(
                    extension_button_id(&extension.id, ExtensionOperation::Install),
                    "Install",
                ),
                configure: None,
                upgrade: None,
            };
        }

        let is_configurable = extension
            .manifest
            .provides
            .contains(&ExtensionProvides::ContextServers);

        match status.clone() {
            ExtensionStatus::NotInstalled => ExtensionCardButtons {
                install_or_uninstall: Button::new(
                    extension_button_id(&extension.id, ExtensionOperation::Install),
                    "Install",
                )
                .style(ButtonStyle::Tinted(ui::TintColor::Accent))
                .start_icon(
                    Icon::new(IconName::Download)
                        .size(IconSize::Small)
                        .color(Color::Muted),
                )
                .on_click({
                    let extension_id = extension.id.clone();
                    move |_, _, cx| {
                        telemetry::event!("Extension Installed");
                        ExtensionStore::global(cx).update(cx, |store, cx| {
                            store.install_latest_extension(extension_id.clone(), cx)
                        });
                    }
                }),
                configure: None,
                upgrade: None,
            },
            ExtensionStatus::Installing => ExtensionCardButtons {
                install_or_uninstall: Button::new(
                    extension_button_id(&extension.id, ExtensionOperation::Install),
                    "Install",
                )
                .style(ButtonStyle::Tinted(ui::TintColor::Accent))
                .start_icon(
                    Icon::new(IconName::Download)
                        .size(IconSize::Small)
                        .color(Color::Muted),
                )
                .disabled(true),
                configure: None,
                upgrade: None,
            },
            ExtensionStatus::Upgrading => ExtensionCardButtons {
                install_or_uninstall: Button::new(
                    extension_button_id(&extension.id, ExtensionOperation::Remove),
                    "Uninstall",
                )
                .style(ButtonStyle::OutlinedGhost)
                .disabled(true),
                configure: is_configurable.then(|| {
                    Button::new(
                        SharedString::from(format!("configure-{}", extension.id)),
                        "Configure",
                    )
                    .disabled(true)
                }),
                upgrade: Some(
                    Button::new(
                        extension_button_id(&extension.id, ExtensionOperation::Upgrade),
                        "Upgrade",
                    )
                    .disabled(true),
                ),
            },
            ExtensionStatus::Installed(installed_version) => ExtensionCardButtons {
                install_or_uninstall: Button::new(
                    extension_button_id(&extension.id, ExtensionOperation::Remove),
                    "Uninstall",
                )
                .style(ButtonStyle::OutlinedGhost)
                .on_click({
                    let extension_id = extension.id.clone();
                    move |_, _, cx| {
                        telemetry::event!("Extension Uninstalled", extension_id);
                        ExtensionStore::global(cx).update(cx, |store, cx| {
                            store
                                .uninstall_extension(extension_id.clone(), cx)
                                .detach_and_log_err(cx);
                        });
                    }
                }),
                configure: is_configurable.then(|| {
                    Button::new(
                        SharedString::from(format!("configure-{}", extension.id)),
                        "Configure",
                    )
                    .style(ButtonStyle::OutlinedGhost)
                    .on_click({
                        let extension_id = extension.id.clone();
                        move |_, _, cx| {
                            if let Some(manifest) = ExtensionStore::global(cx)
                                .read(cx)
                                .extension_manifest_for_id(&extension_id)
                                .cloned()
                                && let Some(events) = extension::ExtensionEvents::try_global(cx)
                            {
                                events.update(cx, |this, cx| {
                                    this.emit(
                                        extension::Event::ConfigureExtensionRequested(manifest),
                                        cx,
                                    )
                                });
                            }
                        }
                    })
                }),
                upgrade: if installed_version == extension.manifest.version {
                    None
                } else {
                    Some(
                        Button::new(extension_button_id(&extension.id, ExtensionOperation::Upgrade), "Upgrade")
                          .style(ButtonStyle::Tinted(ui::TintColor::Accent))
                            .when(!is_compatible, |upgrade_button| {
                                upgrade_button.disabled(true).tooltip({
                                    let version = extension.manifest.version.clone();
                                    move |_, cx| {
                                        Tooltip::simple(
                                            format!(
                                                "v{version} is not compatible with this version of Mav.",
                                            ),
                                             cx,
                                        )
                                    }
                                })
                            })
                            .disabled(!is_compatible)
                            .on_click({
                                let extension_id = extension.id.clone();
                                let version = extension.manifest.version.clone();
                                move |_, _, cx| {
                                    telemetry::event!("Extension Installed", extension_id, version);
                                    ExtensionStore::global(cx).update(cx, |store, cx| {
                                        store
                                            .upgrade_extension(
                                                extension_id.clone(),
                                                version.clone(),
                                                cx,
                                            )
                                            .detach_and_log_err(cx)
                                    });
                                }
                            }),
                    )
                },
            },
            ExtensionStatus::Removing => ExtensionCardButtons {
                install_or_uninstall: Button::new(
                    extension_button_id(&extension.id, ExtensionOperation::Remove),
                    "Uninstall",
                )
                .style(ButtonStyle::OutlinedGhost)
                .disabled(true),
                configure: is_configurable.then(|| {
                    Button::new(
                        SharedString::from(format!("configure-{}", extension.id)),
                        "Configure",
                    )
                    .disabled(true)
                }),
                upgrade: None,
            },
        }
    }
}
