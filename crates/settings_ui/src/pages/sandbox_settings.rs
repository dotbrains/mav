use std::path::PathBuf;

use agent_settings::AgentSettings;
use gpui::{ScrollHandle, prelude::*};
use settings::Settings as _;
use ui::{Banner, Divider, Severity, SwitchField, ToggleState, Tooltip, prelude::*};
use util::ResultExt as _;

use crate::SettingsWindow;
use crate::components::{SettingsInputField, SettingsSectionHeader};

mod store;

use store::{
    add_network_host, add_write_path, canonicalize_host, raw_sandbox_lists, remove_network_host,
    remove_write_path, set_allow_all_hosts, set_allow_fs_write_all, set_allow_git_access,
    set_sandbox_enabled, update_network_host, update_write_path,
};

const SANDBOX_DISCLAIMER: &str = "Customize how the sandbox for the agents tool should behave.";

const DOMAINS_DESCRIPTION: &str = "Each entry is an exact domain (github.com) or a leading-*. subdomain wildcard (*.npmjs.org). IP addresses and local domains are not allowed.";

const WRITE_PATHS_DESCRIPTION: &str =
    "Each entry must be an absolute path and grants write access to the whole subtree.";

pub(crate) fn render_sandbox_settings_page(
    settings_window: &SettingsWindow,
    scroll_handle: &ScrollHandle,
    _window: &mut Window,
    cx: &mut Context<SettingsWindow>,
) -> AnyElement {
    // Sandbox permissions are a user-level setting; they aren't configurable
    // per-project, so always operate against the global value here.
    let permissions = AgentSettings::get_global(cx).sandbox_permissions.clone();
    let validation_error = settings_window.sandbox_host_validation_error.clone();

    // Read the list values from the raw user settings content rather than the
    // compiled `AgentSettings`. The compiled `write_paths` are lexically
    // normalized (see `compile_sandbox_permissions`), so editing or removing a
    // row by the normalized value would fail to match the literal entry stored
    // in settings.json and silently leave the permission in place.
    let (network_hosts, write_paths) = raw_sandbox_lists(cx);

    let host_rows: Vec<AnyElement> = network_hosts
        .into_iter()
        .enumerate()
        .map(|(index, host)| render_host_row(index, host, cx))
        .collect();
    let add_host_input = render_add_host_input(cx);

    let path_rows: Vec<AnyElement> = write_paths
        .into_iter()
        .enumerate()
        .map(|(index, path)| render_path_row(index, path, cx))
        .collect();
    let add_path_input = render_add_path_input(cx);

    let empty_border = cx.theme().colors().border_variant;
    let sandbox_enabled = !permissions.allow_unsandboxed;

    v_flex()
        .id("sandbox-settings-page")
        .size_full()
        .pt_2p5()
        .px_8()
        .pb_16()
        .gap_4()
        .overflow_y_scroll()
        .track_scroll(scroll_handle)
        .child(
            Banner::new().child(
                Label::new(SANDBOX_DISCLAIMER)
                    .size(LabelSize::Small)
                    .color(Color::Muted)
                    .mt_0p5(),
            ),
        )
        .child(
            SwitchField::new(
                "sandbox-enabled",
                Some("Enable Sandbox"),
                Some(
                    "Wrap agent-run terminal commands in an OS-level sandbox. When off, commands run with Mav's own permissions."
                        .into(),
                ),
                sandbox_enabled,
                move |state, _window, cx| {
                    set_sandbox_enabled(*state == ToggleState::Selected, cx);
                },
            )
            .tab_index(0),
        )
        .when(sandbox_enabled, |this| this
        .when_some(validation_error, |this, error| {
            this.child(
                Banner::new()
                    .severity(Severity::Warning)
                    .child(Label::new(error).size(LabelSize::Small))
                    .action_slot(
                        Button::new("dismiss-sandbox-host-error", "Dismiss")
                            .style(ButtonStyle::Tinted(ui::TintColor::Warning))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.sandbox_host_validation_error = None;
                                cx.notify();
                            })),
                    ),
            )
        })
        .child(
            v_flex()
                .gap_3()
                .child(SettingsSectionHeader::new("Network").no_padding(true))
                .child(
                    SwitchField::new(
                        "sandbox-allow-all-hosts",
                        Some("Allow All Domains"),
                        Some(
                            "Let sandboxed commands reach any domain over the network without prompting."
                                .into(),
                        ),
                        permissions.allow_all_hosts,
                        move |state, _window, cx| {
                            set_allow_all_hosts(*state == ToggleState::Selected, cx);
                        },
                    )
                    .tab_index(0),
                )
                .child(render_list_section(
                    "Allowed Domains",
                    DOMAINS_DESCRIPTION,
                    host_rows,
                    add_host_input,
                    empty_border,
                )),
        )
        .child(Divider::horizontal())
        .child(
            v_flex()
                .gap_3()
                .child(SettingsSectionHeader::new("Git").no_padding(true))
                .child(
                    SwitchField::new(
                        "sandbox-allow-git-access",
                        Some("Allow Git Metadata Access"),
                        Some(
                            "Let sandboxed commands access protected Git metadata, including .git directories and linked worktree metadata, without prompting."
                                .into(),
                        ),
                        permissions.allow_git_access,
                        move |state, _window, cx| {
                            set_allow_git_access(*state == ToggleState::Selected, cx);
                        },
                    )
                    .tab_index(0),
                ),
        )
        .child(Divider::horizontal())
        .child(
            v_flex()
                .gap_3()
                .child(SettingsSectionHeader::new("Filesystem").no_padding(true))
                .child(
                    SwitchField::new(
                        "sandbox-allow-fs-write-all",
                        Some("Allow All Filesystem Writes"),
                        Some(
                            "Let sandboxed commands write anywhere on the filesystem without prompting."
                                .into(),
                        ),
                        permissions.allow_fs_write_all,
                        move |state, _window, cx| {
                            set_allow_fs_write_all(*state == ToggleState::Selected, cx);
                        },
                    )
                    .tab_index(0),
                )
                .child(render_list_section(
                    "Writable Paths",
                    WRITE_PATHS_DESCRIPTION,
                    path_rows,
                    add_path_input,
                    empty_border,
                )),
        )
        )
        .into_any_element()
}

fn render_list_section(
    title: &'static str,
    description: &'static str,
    rows: Vec<AnyElement>,
    add_input: AnyElement,
    empty_border: gpui::Hsla,
) -> impl IntoElement {
    let is_empty = rows.is_empty();

    v_flex()
        .gap_0p5()
        .child(Label::new(title))
        .child(
            Label::new(description)
                .size(LabelSize::Small)
                .color(Color::Muted),
        )
        .child(
            v_flex()
                .mt_2()
                .w_full()
                .gap_1p5()
                .when(is_empty, |this| {
                    this.child(render_empty_state(empty_border))
                })
                .when(!is_empty, |this| {
                    this.child(v_flex().gap_1p5().children(rows))
                })
                .child(add_input),
        )
}

fn render_empty_state(border_color: gpui::Hsla) -> AnyElement {
    h_flex()
        .p_2()
        .rounded_md()
        .border_1()
        .border_dashed()
        .border_color(border_color)
        .child(
            Label::new("Nothing configured")
                .size(LabelSize::Small)
                .color(Color::Disabled),
        )
        .into_any_element()
}

fn render_host_row(index: usize, host: String, cx: &mut Context<SettingsWindow>) -> AnyElement {
    let host_for_delete = host.clone();
    let host_for_update = host.clone();
    let settings_window = cx.entity().downgrade();

    SettingsInputField::new(format!("sandbox-host-{}", index))
        .with_initial_text(host)
        .tab_index(0)
        .with_buffer_font()
        .color(Color::Default)
        .action_slot(
            IconButton::new(format!("sandbox-host-delete-{}", index), IconName::Trash)
                .icon_size(IconSize::Small)
                .icon_color(Color::Muted)
                .tooltip(Tooltip::text("Remove Domain"))
                .on_click(cx.listener(move |_, _, _, cx| {
                    remove_network_host(host_for_delete.clone(), cx);
                })),
        )
        .on_confirm(move |new_host, _window, cx| {
            let Some(new_host) = new_host else {
                return;
            };
            let new_host = new_host.trim().to_string();
            if new_host.is_empty() || new_host == host_for_update {
                return;
            }
            let result = canonicalize_host(&new_host);
            settings_window
                .update(cx, |this, cx| {
                    match result {
                        Ok(canonical) => {
                            this.sandbox_host_validation_error = None;
                            update_network_host(host_for_update.clone(), canonical, cx);
                        }
                        Err(error) => {
                            this.sandbox_host_validation_error = Some(error);
                        }
                    }
                    cx.notify();
                })
                .log_err();
        })
        .into_any_element()
}

fn render_add_host_input(cx: &mut Context<SettingsWindow>) -> AnyElement {
    let settings_window = cx.entity().downgrade();

    SettingsInputField::new("sandbox-host-new")
        .with_placeholder("Add domain (e.g. github.com or *.npmjs.org)…")
        .tab_index(0)
        .with_buffer_font()
        .display_clear_button()
        .display_confirm_button()
        .clear_on_confirm()
        .on_confirm(move |host, _window, cx| {
            let Some(host) = host else {
                return;
            };
            let host = host.trim().to_string();
            if host.is_empty() {
                return;
            }
            let result = canonicalize_host(&host);
            settings_window
                .update(cx, |this, cx| {
                    match result {
                        Ok(canonical) => {
                            this.sandbox_host_validation_error = None;
                            add_network_host(canonical, cx);
                        }
                        Err(error) => {
                            this.sandbox_host_validation_error = Some(error);
                        }
                    }
                    cx.notify();
                })
                .log_err();
        })
        .into_any_element()
}

fn render_path_row(index: usize, path: PathBuf, cx: &mut Context<SettingsWindow>) -> AnyElement {
    let path_for_delete = path.clone();
    let path_for_update = path.clone();
    let settings_window = cx.entity().downgrade();

    SettingsInputField::new(format!("sandbox-path-{}", index))
        .with_initial_text(path.to_string_lossy().into_owned())
        .tab_index(0)
        .with_buffer_font()
        .color(Color::Default)
        .action_slot(
            IconButton::new(format!("sandbox-path-delete-{}", index), IconName::Trash)
                .icon_size(IconSize::Small)
                .icon_color(Color::Muted)
                .tooltip(Tooltip::text("Remove Path"))
                .on_click(cx.listener(move |_, _, _, cx| {
                    remove_write_path(path_for_delete.clone(), cx);
                })),
        )
        .on_confirm(move |new_path, _window, cx| {
            let Some(new_path) = new_path else {
                return;
            };
            let new_path = new_path.trim();
            if new_path.is_empty() {
                return;
            }
            let new_path = PathBuf::from(new_path);
            if new_path == path_for_update {
                return;
            }
            update_write_path(path_for_update.clone(), new_path, cx);
            settings_window.update(cx, |_, cx| cx.notify()).log_err();
        })
        .into_any_element()
}

fn render_add_path_input(cx: &mut Context<SettingsWindow>) -> AnyElement {
    let settings_window = cx.entity().downgrade();

    SettingsInputField::new("sandbox-path-new")
        .with_placeholder("Add an absolute path (e.g. /path/to/directory)…")
        .tab_index(0)
        .with_buffer_font()
        .display_clear_button()
        .display_confirm_button()
        .clear_on_confirm()
        .on_confirm(move |path, _window, cx| {
            let Some(path) = path else {
                return;
            };
            let path = path.trim();
            if path.is_empty() {
                return;
            }
            add_write_path(PathBuf::from(path), cx);
            settings_window.update(cx, |_, cx| cx.notify()).log_err();
        })
        .into_any_element()
}
