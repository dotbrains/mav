use super::*;

pub fn set_completion_provider(fs: Arc<dyn Fs>, cx: &mut App, provider: EditPredictionProvider) {
    update_settings_file(fs, cx, move |settings, _| {
        settings
            .project
            .all_languages
            .edit_predictions
            .get_or_insert_default()
            .provider = Some(provider);
    });
}

pub fn get_available_providers(cx: &mut App) -> Vec<EditPredictionProvider> {
    let mut providers = Vec::new();

    providers.push(EditPredictionProvider::Mav);

    let app_state = workspace::AppState::global(cx);
    if copilot::GlobalCopilotAuth::try_get_or_init(app_state, cx)
        .is_some_and(|copilot| copilot.0.read(cx).is_authenticated())
    {
        providers.push(EditPredictionProvider::Copilot);
    };

    if codestral::codestral_api_key(cx).is_some() {
        providers.push(EditPredictionProvider::Codestral);
    }

    if edit_prediction::ollama::is_available(cx) {
        providers.push(EditPredictionProvider::Ollama);
    }

    if all_language_settings(None, cx)
        .edit_predictions
        .open_ai_compatible_api
        .is_some()
    {
        providers.push(EditPredictionProvider::OpenAiCompatibleApi);
    }

    if edit_prediction::mercury::mercury_api_token(cx)
        .read(cx)
        .has_key()
    {
        providers.push(EditPredictionProvider::Mercury);
    }

    providers
}

pub(crate) fn toggle_show_edit_predictions_for_language(
    language: Arc<Language>,
    fs: Arc<dyn Fs>,
    cx: &mut App,
) {
    let show_edit_predictions =
        all_language_settings(None, cx).show_edit_predictions(Some(&language), cx);
    update_settings_file(fs, cx, move |settings, _| {
        settings
            .project
            .all_languages
            .languages
            .0
            .entry(language.name().0.to_string())
            .or_default()
            .show_edit_predictions = Some(!show_edit_predictions);
    });
}

pub(crate) fn hide_copilot(fs: Arc<dyn Fs>, cx: &mut App) {
    update_settings_file(fs, cx, move |settings, _| {
        settings
            .project
            .all_languages
            .edit_predictions
            .get_or_insert(Default::default())
            .provider = Some(EditPredictionProvider::None);
    });
}

pub(crate) fn toggle_edit_prediction_mode(
    fs: Arc<dyn Fs>,
    mode: EditPredictionsMode,
    cx: &mut App,
) {
    let settings = AllLanguageSettings::get_global(cx);
    let current_mode = settings.edit_predictions_mode();

    if current_mode != mode {
        update_settings_file(fs, cx, move |settings, _cx| {
            if let Some(edit_predictions) = settings.project.all_languages.edit_predictions.as_mut()
            {
                edit_predictions.mode = Some(mode);
            } else {
                settings.project.all_languages.edit_predictions =
                    Some(settings::EditPredictionSettingsContent {
                        mode: Some(mode),
                        ..Default::default()
                    });
            }
        });
    }
}

pub(crate) fn render_zeta_tab_animation(cx: &App) -> impl IntoElement {
    let tab = |n: u64, inverted: bool| {
        let text_color = cx.theme().colors().text;

        h_flex().child(
            h_flex()
                .text_size(TextSize::XSmall.rems(cx))
                .text_color(text_color)
                .child("tab")
                .with_animation(
                    ElementId::Integer(n),
                    Animation::new(Duration::from_secs(3)).repeat(),
                    move |tab, delta| {
                        let n_f32 = n as f32;

                        let offset = if inverted {
                            0.2 * (4.0 - n_f32)
                        } else {
                            0.2 * n_f32
                        };

                        let phase = (delta - offset + 1.0) % 1.0;
                        let pulse = if phase < 0.6 {
                            let t = phase / 0.6;
                            1.0 - (0.5 - t).abs() * 2.0
                        } else {
                            0.0
                        };

                        let eased = ease_in_out(pulse);
                        let opacity = 0.1 + 0.5 * eased;

                        tab.text_color(text_color.opacity(opacity))
                    },
                ),
        )
    };

    let tab_sequence = |inverted: bool| {
        h_flex()
            .gap_1()
            .child(tab(0, inverted))
            .child(tab(1, inverted))
            .child(tab(2, inverted))
            .child(tab(3, inverted))
            .child(tab(4, inverted))
    };

    h_flex()
        .my_1p5()
        .p_4()
        .justify_center()
        .gap_2()
        .rounded_xs()
        .border_1()
        .border_dashed()
        .border_color(cx.theme().colors().border)
        .bg(gpui::pattern_slash(
            cx.theme().colors().border.opacity(0.5),
            1.,
            8.,
        ))
        .child(tab_sequence(true))
        .child(Icon::new(IconName::MavPredict))
        .child(tab_sequence(false))
}

pub(crate) fn emit_edit_prediction_menu_opened(
    provider: &str,
    file: &Option<Arc<dyn File>>,
    language: &Option<Arc<Language>>,
    project: &WeakEntity<Project>,
    cx: &App,
) {
    let language_name = language.as_ref().map(|l| l.name());
    let edit_predictions_enabled_for_language =
        LanguageSettings::resolve(None, language_name.as_ref(), cx).show_edit_predictions;
    let file_extension = file
        .as_ref()
        .and_then(|f| {
            std::path::Path::new(f.file_name(cx))
                .extension()
                .and_then(|e| e.to_str())
        })
        .map(|s| s.to_string());
    let is_via_ssh = project
        .upgrade()
        .map(|p| p.read(cx).is_via_remote_server())
        .unwrap_or(false);
    telemetry::event!(
        "Toolbar Menu Opened",
        name = "Edit Predictions",
        provider,
        file_extension,
        edit_predictions_enabled_for_language,
        is_via_ssh,
    );
}

pub(crate) fn copilot_settings_url(enterprise_uri: Option<&str>) -> Arc<str> {
    match enterprise_uri {
        Some(uri) => format!("{}{}", uri.trim_end_matches('/'), COPILOT_SETTINGS_PATH).into(),
        None => COPILOT_SETTINGS_URL.into(),
    }
}
