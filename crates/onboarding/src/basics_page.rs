mod agents;
mod theme;

use client::{TelemetrySettings, UserStore};
use fs::Fs;
use gpui::{Action, App, Entity, IntoElement};
use project::project_settings::ProjectSettings;
use settings::{BaseKeymap, Settings, update_settings_file};
use ui::{
    Divider, SwitchField, TintColor, ToggleButtonGroup, ToggleButtonWithIcon, Tooltip, prelude::*,
};
use vim_mode_setting::VimModeSetting;

use crate::{ImportCursorSettings, ImportVsCodeSettings, SettingsImportState};
pub(crate) use agents::FEATURED_AGENT_IDS;
use agents::render_ai_section;
use theme::render_theme_section;

fn render_telemetry_section(tab_index: &mut isize, cx: &App) -> impl IntoElement {
    let fs = <dyn Fs>::global(cx);

    v_flex()
        .gap_4()
        .child(
            SwitchField::new(
                "onboarding-telemetry-metrics",
                None::<&str>,
                Some("Help improve Mav by sending anonymous usage data".into()),
                if TelemetrySettings::get_global(cx).metrics {
                    ui::ToggleState::Selected
                } else {
                    ui::ToggleState::Unselected
                },
                {
                    let fs = fs.clone();
                    move |selection, _, cx| {
                        let enabled = match selection {
                            ToggleState::Selected => true,
                            ToggleState::Unselected => false,
                            ToggleState::Indeterminate => {
                                return;
                            }
                        };

                        update_settings_file(fs.clone(), cx, move |setting, _| {
                            setting.telemetry.get_or_insert_default().metrics = Some(enabled);
                        });

                        // This telemetry event shouldn't fire when it's off. If it does we'll be alerted
                        // and can fix it in a timely manner to respect a user's choice.
                        telemetry::event!(
                            "Welcome Page Telemetry Metrics Toggled",
                            options = if enabled { "on" } else { "off" }
                        );
                    }
                },
            )
            .tab_index({
                *tab_index += 1;
                *tab_index
            }),
        )
        .child(
            SwitchField::new(
                "onboarding-telemetry-crash-reports",
                None::<&str>,
                Some(
                    "Help fix Mav by sending crash reports so we can fix critical issues fast"
                        .into(),
                ),
                if TelemetrySettings::get_global(cx).diagnostics {
                    ui::ToggleState::Selected
                } else {
                    ui::ToggleState::Unselected
                },
                {
                    let fs = fs.clone();
                    move |selection, _, cx| {
                        let enabled = match selection {
                            ToggleState::Selected => true,
                            ToggleState::Unselected => false,
                            ToggleState::Indeterminate => {
                                return;
                            }
                        };

                        update_settings_file(fs.clone(), cx, move |setting, _| {
                            setting.telemetry.get_or_insert_default().diagnostics = Some(enabled);
                        });

                        // This telemetry event shouldn't fire when it's off. If it does we'll be alerted
                        // and can fix it in a timely manner to respect a user's choice.
                        telemetry::event!(
                            "Welcome Page Telemetry Diagnostics Toggled",
                            options = if enabled { "on" } else { "off" }
                        );
                    }
                },
            )
            .tab_index({
                *tab_index += 1;
                *tab_index
            }),
        )
}

fn render_base_keymap_section(tab_index: &mut isize, cx: &mut App) -> impl IntoElement {
    let base_keymap = match BaseKeymap::get_global(cx) {
        BaseKeymap::VSCode => Some(0),
        BaseKeymap::JetBrains => Some(1),
        BaseKeymap::SublimeText => Some(2),
        BaseKeymap::Atom => Some(3),
        BaseKeymap::Emacs => Some(4),
        BaseKeymap::Cursor => Some(5),
        BaseKeymap::TextMate | BaseKeymap::None => None,
    };

    return v_flex().gap_2().child(Label::new("Base Keymap")).child(
        ToggleButtonGroup::two_rows(
            "base_keymap_selection",
            [
                ToggleButtonWithIcon::new("VS Code", IconName::EditorVsCode, |_, _, cx| {
                    write_keymap_base(BaseKeymap::VSCode, cx);
                }),
                ToggleButtonWithIcon::new("JetBrains", IconName::EditorJetBrains, |_, _, cx| {
                    write_keymap_base(BaseKeymap::JetBrains, cx);
                }),
                ToggleButtonWithIcon::new("Sublime Text", IconName::EditorSublime, |_, _, cx| {
                    write_keymap_base(BaseKeymap::SublimeText, cx);
                }),
            ],
            [
                ToggleButtonWithIcon::new("Atom", IconName::EditorAtom, |_, _, cx| {
                    write_keymap_base(BaseKeymap::Atom, cx);
                }),
                ToggleButtonWithIcon::new("Emacs", IconName::EditorEmacs, |_, _, cx| {
                    write_keymap_base(BaseKeymap::Emacs, cx);
                }),
                ToggleButtonWithIcon::new("Cursor", IconName::EditorCursor, |_, _, cx| {
                    write_keymap_base(BaseKeymap::Cursor, cx);
                }),
            ],
        )
        .when_some(base_keymap, |this, base_keymap| {
            this.selected_index(base_keymap)
        })
        .full_width()
        .tab_index(tab_index)
        .size(ui::ToggleButtonGroupSize::Medium)
        .style(ui::ToggleButtonGroupStyle::Outlined),
    );

    fn write_keymap_base(keymap_base: BaseKeymap, cx: &App) {
        let fs = <dyn Fs>::global(cx);

        update_settings_file(fs, cx, move |setting, _| {
            setting.base_keymap = Some(keymap_base.into());
        });

        telemetry::event!("Welcome Keymap Changed", keymap = keymap_base);
    }
}

fn render_vim_mode_switch(tab_index: &mut isize, cx: &mut App) -> impl IntoElement {
    let toggle_state = if VimModeSetting::get_global(cx).0 {
        ui::ToggleState::Selected
    } else {
        ui::ToggleState::Unselected
    };
    SwitchField::new(
        "onboarding-vim-mode",
        Some("Vim Mode"),
        Some("Coming from Neovim? Use our first-class implementation of Vim Mode".into()),
        toggle_state,
        {
            let fs = <dyn Fs>::global(cx);
            move |&selection, _, cx| {
                let vim_mode = match selection {
                    ToggleState::Selected => true,
                    ToggleState::Unselected => false,
                    ToggleState::Indeterminate => {
                        return;
                    }
                };
                update_settings_file(fs.clone(), cx, move |setting, _| {
                    setting.vim_mode = Some(vim_mode);
                });

                telemetry::event!(
                    "Welcome Vim Mode Toggled",
                    options = if vim_mode { "on" } else { "off" },
                );
            }
        },
    )
    .tab_index({
        *tab_index += 1;
        *tab_index - 1
    })
}

fn render_worktree_auto_trust_switch(tab_index: &mut isize, cx: &mut App) -> impl IntoElement {
    let toggle_state = if ProjectSettings::get_global(cx).session.trust_all_worktrees {
        ui::ToggleState::Selected
    } else {
        ui::ToggleState::Unselected
    };

    let tooltip_description = "Mav can only allow services like language servers, project settings, and MCP servers to run after you mark a new project as trusted.";

    SwitchField::new(
        "onboarding-auto-trust-worktrees",
        Some("Trust All Projects By Default"),
        Some("Automatically mark all new projects as trusted to unlock all Mav's features".into()),
        toggle_state,
        {
            let fs = <dyn Fs>::global(cx);
            move |&selection, _, cx| {
                let trust = match selection {
                    ToggleState::Selected => true,
                    ToggleState::Unselected => false,
                    ToggleState::Indeterminate => {
                        return;
                    }
                };
                update_settings_file(fs.clone(), cx, move |setting, _| {
                    setting.session.get_or_insert_default().trust_all_worktrees = Some(trust);
                });

                telemetry::event!(
                    "Welcome Page Worktree Auto Trust Toggled",
                    options = if trust { "on" } else { "off" }
                );
            }
        },
    )
    .tab_index({
        *tab_index += 1;
        *tab_index - 1
    })
    .tooltip(Tooltip::text(tooltip_description))
}

fn render_setting_import_button(
    tab_index: isize,
    label: SharedString,
    action: &dyn Action,
    imported: bool,
) -> impl IntoElement + 'static {
    let action = action.boxed_clone();

    Button::new(label.clone(), label.clone())
        .style(ButtonStyle::OutlinedGhost)
        .size(ButtonSize::Medium)
        .label_size(LabelSize::Small)
        .selected_style(ButtonStyle::Tinted(TintColor::Accent))
        .toggle_state(imported)
        .tab_index(tab_index)
        .when(imported, |this| {
            this.end_icon(Icon::new(IconName::Check).size(IconSize::Small))
                .color(Color::Success)
        })
        .on_click(move |_, window, cx| {
            telemetry::event!("Welcome Import Settings", import_source = label,);
            window.dispatch_action(action.boxed_clone(), cx);
        })
}

fn render_import_settings_section(tab_index: &mut isize, cx: &mut App) -> impl IntoElement {
    let import_state = SettingsImportState::global(cx);
    let imports: [(SharedString, &dyn Action, bool); 2] = [
        (
            "VS Code".into(),
            &ImportVsCodeSettings { skip_prompt: false },
            import_state.vscode,
        ),
        (
            "Cursor".into(),
            &ImportCursorSettings { skip_prompt: false },
            import_state.cursor,
        ),
    ];

    let [vscode, cursor] = imports.map(|(label, action, imported)| {
        *tab_index += 1;
        render_setting_import_button(*tab_index - 1, label, action, imported)
    });

    h_flex()
        .gap_2()
        .flex_wrap()
        .justify_between()
        .child(
            v_flex()
                .gap_0p5()
                .max_w_5_6()
                .child(Label::new("Import Settings"))
                .child(
                    Label::new("Automatically pull your settings from other editors")
                        .color(Color::Muted),
                ),
        )
        .child(h_flex().gap_1().child(vscode).child(cursor))
}

pub(crate) fn render_basics_page(user_store: &Entity<UserStore>, cx: &mut App) -> impl IntoElement {
    let mut tab_index = 0;

    v_flex()
        .id("basics-page")
        .gap_6()
        .child(render_theme_section(&mut tab_index, cx))
        .child(render_base_keymap_section(&mut tab_index, cx))
        .child(render_ai_section(user_store, cx))
        .child(render_import_settings_section(&mut tab_index, cx))
        .child(render_vim_mode_switch(&mut tab_index, cx))
        .child(render_worktree_auto_trust_switch(&mut tab_index, cx))
        .child(Divider::horizontal().color(ui::DividerColor::BorderVariant))
        .child(render_telemetry_section(&mut tab_index, cx))
}
