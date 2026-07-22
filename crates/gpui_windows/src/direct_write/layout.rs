use super::*;

impl DirectWriteState {
    pub(super) fn layout_line(
        &mut self,
        components: &DirectWriteComponents,
        text: &str,
        font_size: Pixels,
        font_runs: &[FontRun],
    ) -> Result<LineLayout> {
        if font_runs.is_empty() {
            return Ok(LineLayout {
                font_size,
                ..Default::default()
            });
        }
        unsafe {
            self.layout_line_scratch.clear();
            self.layout_line_scratch.extend(text.encode_utf16());
            let text_wide = &*self.layout_line_scratch;

            let mut utf8_offset = 0usize;
            let mut utf16_offset = 0u32;
            let text_layout = {
                let first_run = &font_runs[0];
                let font_info = &self.fonts[first_run.font_id.0];
                let collection = &font_info.font_collection;
                let format: IDWriteTextFormat1 = components
                    .factory
                    .CreateTextFormat(
                        &font_info.font_family_h,
                        collection,
                        font_info.font_face.GetWeight(),
                        font_info.font_face.GetStyle(),
                        DWRITE_FONT_STRETCH_NORMAL,
                        font_size.as_f32(),
                        &components.locale,
                    )?
                    .cast()?;
                if let Some(ref fallbacks) = font_info.fallbacks {
                    format.SetFontFallback(fallbacks)?;
                }

                let layout = components.factory.CreateTextLayout(
                    text_wide,
                    &format,
                    f32::INFINITY,
                    f32::INFINITY,
                )?;
                let current_text = &text[utf8_offset..(utf8_offset + first_run.len)];
                utf8_offset += first_run.len;
                let current_text_utf16_length = current_text.encode_utf16().count() as u32;
                let text_range = DWRITE_TEXT_RANGE {
                    startPosition: utf16_offset,
                    length: current_text_utf16_length,
                };
                layout.SetTypography(&font_info.features, text_range)?;
                utf16_offset += current_text_utf16_length;

                layout
            };

            let (ascent, descent) = {
                let mut first_metrics = [DWRITE_LINE_METRICS::default(); 4];
                let mut line_count = 0u32;
                text_layout.GetLineMetrics(Some(&mut first_metrics), &mut line_count)?;
                (
                    px(first_metrics[0].baseline),
                    px(first_metrics[0].height - first_metrics[0].baseline),
                )
            };
            let mut break_ligatures = true;
            for run in &font_runs[1..] {
                let font_info = &self.fonts[run.font_id.0];
                let current_text = &text[utf8_offset..(utf8_offset + run.len)];
                utf8_offset += run.len;
                let current_text_utf16_length = current_text.encode_utf16().count() as u32;

                let collection = &font_info.font_collection;
                let text_range = DWRITE_TEXT_RANGE {
                    startPosition: utf16_offset,
                    length: current_text_utf16_length,
                };
                utf16_offset += current_text_utf16_length;
                text_layout.SetFontCollection(collection, text_range)?;
                text_layout.SetFontFamilyName(&font_info.font_family_h, text_range)?;
                let font_size = if break_ligatures {
                    font_size.as_f32().next_up()
                } else {
                    font_size.as_f32()
                };
                text_layout.SetFontSize(font_size, text_range)?;
                text_layout.SetFontStyle(font_info.font_face.GetStyle(), text_range)?;
                text_layout.SetFontWeight(font_info.font_face.GetWeight(), text_range)?;
                text_layout.SetTypography(&font_info.features, text_range)?;

                break_ligatures = !break_ligatures;
            }

            let mut runs = Vec::new();
            let renderer_context = RendererContext {
                text_system: self,
                components,
                index_converter: StringIndexConverter::new(text),
                runs: &mut runs,
                width: 0.0,
            };
            text_layout.Draw(
                Some((&raw const renderer_context).cast::<c_void>()),
                &components.text_renderer.0,
                0.0,
                0.0,
            )?;
            let width = px(renderer_context.width);

            Ok(LineLayout {
                font_size,
                width,
                ascent,
                descent,
                runs,
                len: text.len(),
            })
        }
    }
}
