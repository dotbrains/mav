const LIGHT_THEMES: [&str; 3] = ["One Light", "Ayu Light", "Gruvbox Light"];
const DARK_THEMES: [&str; 3] = ["One Dark", "Ayu Dark", "Gruvbox Dark"];
const FAMILY_NAMES: [SharedString; 3] = [
    SharedString::new_static("One"),
    SharedString::new_static("Ayu"),
    SharedString::new_static("Gruvbox"),
];

fn get_theme_family_themes(theme_name: &str) -> Option<(&'static str, &'static str)> {
    for i in 0..LIGHT_THEMES.len() {
        if LIGHT_THEMES[i] == theme_name || DARK_THEMES[i] == theme_name {
            return Some((LIGHT_THEMES[i], DARK_THEMES[i]));
        }
    }
    None
}

pub(super) fn render_theme_section(tab_index: &mut isize, cx: &mut App) -> impl IntoElement {
    let theme_selection = ThemeSettings::get_global(cx).theme.clone();
    let system_appearance = SystemAppearance::global(cx);

    let theme_mode = theme_selection
        .mode()
        .unwrap_or_else(|| match *system_appearance {
            Appearance::Light => ThemeAppearanceMode::Light,
            Appearance::Dark => ThemeAppearanceMode::Dark,
        });

    return v_flex()
        .gap_2()
        .child(
            h_flex().justify_between().child(Label::new("Theme")).child(
                ToggleButtonGroup::single_row(
                    "theme-selector-onboarding-dark-light",
                    [
                        ThemeAppearanceMode::Light,
                        ThemeAppearanceMode::Dark,
                        ThemeAppearanceMode::System,
                    ]
                    .map(|mode| {
                        const MODE_NAMES: [SharedString; 3] = [
                            SharedString::new_static("Light"),
                            SharedString::new_static("Dark"),
                            SharedString::new_static("System"),
                        ];
                        ToggleButtonSimple::new(
                            MODE_NAMES[mode as usize].clone(),
                            move |_, _, cx| {
                                write_mode_change(mode, cx);

                                telemetry::event!(
                                    "Welcome Theme mode Changed",
                                    from = theme_mode,
                                    to = mode
                                );
                            },
                        )
                    }),
                )
                .size(ToggleButtonGroupSize::Medium)
                .tab_index(tab_index)
                .selected_index(theme_mode as usize)
                .style(ui::ToggleButtonGroupStyle::Outlined)
                .width(rems_from_px(3. * 64.)),
            ),
        )
        .child(
            h_flex()
                .gap_2()
                .justify_between()
                .children(render_theme_previews(tab_index, &theme_selection, cx)),
        );

    fn render_theme_previews(
        tab_index: &mut isize,
        theme_selection: &ThemeSelection,
        cx: &mut App,
    ) -> [impl IntoElement; 3] {
        let system_appearance = SystemAppearance::global(cx);
        let theme_registry = ThemeRegistry::global(cx);

        let theme_seed = 0xBEEF as f32;
        let theme_mode = theme_selection
            .mode()
            .unwrap_or_else(|| match *system_appearance {
                Appearance::Light => ThemeAppearanceMode::Light,
                Appearance::Dark => ThemeAppearanceMode::Dark,
            });
        let appearance = match theme_mode {
            ThemeAppearanceMode::Light => Appearance::Light,
            ThemeAppearanceMode::Dark => Appearance::Dark,
            ThemeAppearanceMode::System => *system_appearance,
        };
        let current_theme_name: SharedString = theme_selection.name(appearance).0.into();

        let theme_names = match appearance {
            Appearance::Light => LIGHT_THEMES,
            Appearance::Dark => DARK_THEMES,
        };

        let themes = theme_names.map(|theme| theme_registry.get(theme).unwrap());

        [0, 1, 2].map(|index| {
            let theme = &themes[index];
            let is_selected = theme.name == current_theme_name;
            let name = theme.name.clone();
            let colors = cx.theme().colors();

            v_flex()
                .w_full()
                .items_center()
                .gap_1()
                .child(
                    h_flex()
                        .id(name)
                        .relative()
                        .w_full()
                        .border_2()
                        .border_color(colors.border_transparent)
                        .rounded(ThemePreviewTile::ROOT_RADIUS)
                        .map(|this| {
                            if is_selected {
                                this.border_color(colors.border_selected)
                            } else {
                                this.opacity(0.8).hover(|s| s.border_color(colors.border))
                            }
                        })
                        .tab_index({
                            *tab_index += 1;
                            *tab_index - 1
                        })
                        .focus_visible(|mut style| {
                            style.border_color = Some(colors.border_focused);
                            style
                        })
                        .on_click({
                            let theme_name = theme.name.clone();
                            let current_theme_name = current_theme_name.clone();

                            move |_, _, cx| {
                                write_theme_change(theme_name.clone(), theme_mode, cx);
                                telemetry::event!(
                                    "Welcome Theme Changed",
                                    from = current_theme_name,
                                    to = theme_name
                                );
                            }
                        })
                        .map(|this| {
                            if theme_mode == ThemeAppearanceMode::System {
                                let (light, dark) = (
                                    theme_registry.get(LIGHT_THEMES[index]).unwrap(),
                                    theme_registry.get(DARK_THEMES[index]).unwrap(),
                                );
                                this.child(
                                    ThemePreviewTile::new(light, theme_seed)
                                        .style(ThemePreviewStyle::SideBySide(dark)),
                                )
                            } else {
                                this.child(
                                    ThemePreviewTile::new(theme.clone(), theme_seed)
                                        .style(ThemePreviewStyle::Bordered),
                                )
                            }
                        }),
                )
                .child(
                    Label::new(FAMILY_NAMES[index].clone())
                        .color(Color::Muted)
                        .size(LabelSize::Small),
                )
        })
    }

    fn write_mode_change(mode: ThemeAppearanceMode, cx: &mut App) {
        let fs = <dyn Fs>::global(cx);
        update_settings_file(fs, cx, move |settings, _cx| {
            theme_settings::set_mode(settings, mode);
        });
    }

    fn write_theme_change(
        theme: impl Into<Arc<str>>,
        theme_mode: ThemeAppearanceMode,
        cx: &mut App,
    ) {
        let fs = <dyn Fs>::global(cx);
        let theme = theme.into();
        update_settings_file(fs, cx, move |settings, cx| match theme_mode {
            ThemeAppearanceMode::System => {
                let (light_theme, dark_theme) =
                    get_theme_family_themes(&theme).unwrap_or((theme.as_ref(), theme.as_ref()));

                settings.theme.theme = Some(settings::ThemeSelection::Dynamic {
                    mode: ThemeAppearanceMode::System,
                    light: ThemeName(light_theme.into()),
                    dark: ThemeName(dark_theme.into()),
                });
            }
            ThemeAppearanceMode::Light => theme_settings::set_theme(
                settings,
                theme,
                Appearance::Light,
                *SystemAppearance::global(cx),
            ),
            ThemeAppearanceMode::Dark => theme_settings::set_theme(
                settings,
                theme,
                Appearance::Dark,
                *SystemAppearance::global(cx),
            ),
        });
    }
}
use std::sync::Arc;

use ::theme::{Appearance, SystemAppearance, ThemeRegistry};
use fs::Fs;
use gpui::{App, InteractiveElement, IntoElement};
use settings::{Settings, update_settings_file};
use theme_settings::{ThemeAppearanceMode, ThemeName, ThemeSelection, ThemeSettings};
use ui::{
    StatefulInteractiveElement, ToggleButtonGroup, ToggleButtonGroupSize, ToggleButtonSimple,
    prelude::*,
};

use crate::theme_preview::{ThemePreviewStyle, ThemePreviewTile};
