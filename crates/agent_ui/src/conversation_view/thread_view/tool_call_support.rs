use super::*;

impl ThreadView {
    pub(super) fn render_diff_loading(&self, cx: &Context<Self>) -> AnyElement {
        let bar = |n: u64, width_class: &str| {
            let bg_color = cx.theme().colors().element_active;
            let base = h_flex().h_1().rounded_full();

            let modified = match width_class {
                "w_4_5" => base.w_3_4(),
                "w_1_4" => base.w_1_4(),
                "w_2_4" => base.w_2_4(),
                "w_3_5" => base.w_3_5(),
                "w_2_5" => base.w_2_5(),
                _ => base.w_1_2(),
            };

            modified.with_animation(
                ElementId::Integer(n),
                Animation::new(Duration::from_secs(2)).repeat(),
                move |tab, delta| {
                    let delta = (delta - 0.15 * n as f32) / 0.7;
                    let delta = 1.0 - (0.5 - delta).abs() * 2.;
                    let delta = ease_in_out(delta.clamp(0., 1.));
                    let delta = 0.1 + 0.9 * delta;

                    tab.bg(bg_color.opacity(delta))
                },
            )
        };

        v_flex()
            .p_3()
            .gap_1()
            .rounded_b_md()
            .bg(cx.theme().colors().editor_background)
            .child(bar(0, "w_4_5"))
            .child(bar(1, "w_1_4"))
            .child(bar(2, "w_2_4"))
            .child(bar(3, "w_3_5"))
            .child(bar(4, "w_2_5"))
            .into_any_element()
    }

    pub(super) fn tool_card_header_bg(&self, cx: &Context<Self>) -> Hsla {
        cx.theme()
            .colors()
            .element_background
            .blend(cx.theme().colors().editor_foreground.opacity(0.025))
    }

    pub(super) fn tool_card_border_color(&self, cx: &Context<Self>) -> Hsla {
        cx.theme().colors().border.opacity(0.8)
    }

    pub(super) fn tool_name_font_size(&self) -> Rems {
        rems_from_px(13.)
    }

    pub(super) fn provider_by_name(
        name: &SharedString,
        cx: &App,
    ) -> Option<Arc<dyn LanguageModelProvider>> {
        LanguageModelRegistry::read_global(cx)
            .providers()
            .into_iter()
            .find(|provider| provider.name().0 == *name)
    }
}
