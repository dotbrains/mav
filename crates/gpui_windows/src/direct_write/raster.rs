use super::*;

impl DirectWriteState {
    pub(super) fn font_metrics(&self, font_id: FontId) -> FontMetrics {
        unsafe {
            let font_info = &self.fonts[font_id.0];
            let mut metrics = std::mem::zeroed();
            font_info.font_face.GetMetrics(&mut metrics);

            FontMetrics {
                units_per_em: metrics.Base.designUnitsPerEm as _,
                ascent: metrics.Base.ascent as _,
                descent: -(metrics.Base.descent as f32),
                line_gap: metrics.Base.lineGap as _,
                underline_position: metrics.Base.underlinePosition as _,
                underline_thickness: metrics.Base.underlineThickness as _,
                cap_height: metrics.Base.capHeight as _,
                x_height: metrics.Base.xHeight as _,
                bounding_box: Bounds {
                    origin: Point {
                        x: metrics.glyphBoxLeft as _,
                        y: metrics.glyphBoxBottom as _,
                    },
                    size: Size {
                        width: (metrics.glyphBoxRight - metrics.glyphBoxLeft) as _,
                        height: (metrics.glyphBoxTop - metrics.glyphBoxBottom) as _,
                    },
                },
            }
        }
    }

    pub(super) fn create_glyph_run_analysis(
        &self,
        components: &DirectWriteComponents,
        params: &RenderGlyphParams,
    ) -> Result<IDWriteGlyphRunAnalysis> {
        let font = &self.fonts[params.font_id.0];
        let glyph_id = [params.glyph_id.0 as u16];
        let advance = [0.0];
        let offset = [DWRITE_GLYPH_OFFSET::default()];
        let glyph_run = DWRITE_GLYPH_RUN {
            fontFace: ManuallyDrop::new(Some(unsafe { std::ptr::read(&***font.font_face) })),
            fontEmSize: params.font_size.as_f32(),
            glyphCount: 1,
            glyphIndices: glyph_id.as_ptr(),
            glyphAdvances: advance.as_ptr(),
            glyphOffsets: offset.as_ptr(),
            isSideways: BOOL(0),
            bidiLevel: 0,
        };
        let transform = DWRITE_MATRIX {
            m11: params.scale_factor,
            m12: 0.0,
            m21: 0.0,
            m22: params.scale_factor,
            dx: 0.0,
            dy: 0.0,
        };
        let baseline_origin_x =
            params.subpixel_variant.x as f32 / SUBPIXEL_VARIANTS_X as f32 / params.scale_factor;
        let baseline_origin_y = params.subpixel_variant.y as f32
            / gpui::SUBPIXEL_VARIANTS_Y as f32
            / params.scale_factor;

        let mut rendering_mode = DWRITE_RENDERING_MODE1::default();
        let mut grid_fit_mode = DWRITE_GRID_FIT_MODE::default();
        unsafe {
            font.font_face.GetRecommendedRenderingMode(
                params.font_size.as_f32(),
                // Using 96 as scale is applied by the transform
                96.0,
                96.0,
                Some(&transform),
                false,
                DWRITE_OUTLINE_THRESHOLD_ANTIALIASED,
                DWRITE_MEASURING_MODE_NATURAL,
                None,
                &mut rendering_mode,
                &mut grid_fit_mode,
            )?;
        }
        let rendering_mode = match rendering_mode {
            DWRITE_RENDERING_MODE1_OUTLINE => DWRITE_RENDERING_MODE1_NATURAL_SYMMETRIC,
            m => m,
        };

        let antialias_mode = if params.subpixel_rendering {
            DWRITE_TEXT_ANTIALIAS_MODE_CLEARTYPE
        } else {
            DWRITE_TEXT_ANTIALIAS_MODE_GRAYSCALE
        };

        let glyph_analysis = unsafe {
            components.factory.CreateGlyphRunAnalysis(
                &glyph_run,
                Some(&transform),
                rendering_mode,
                DWRITE_MEASURING_MODE_NATURAL,
                grid_fit_mode,
                antialias_mode,
                baseline_origin_x,
                baseline_origin_y,
            )
        }?;
        Ok(glyph_analysis)
    }

    pub(super) fn raster_bounds(
        &self,
        components: &DirectWriteComponents,
        params: &RenderGlyphParams,
    ) -> Result<Bounds<DevicePixels>> {
        let glyph_analysis = self.create_glyph_run_analysis(components, params)?;

        let texture_type = if params.subpixel_rendering {
            DWRITE_TEXTURE_CLEARTYPE_3x1
        } else {
            DWRITE_TEXTURE_ALIASED_1x1
        };

        let bounds = unsafe { glyph_analysis.GetAlphaTextureBounds(texture_type)? };

        if bounds.right < bounds.left {
            Ok(Bounds {
                origin: point(0.into(), 0.into()),
                size: size(0.into(), 0.into()),
            })
        } else {
            Ok(Bounds {
                origin: point(bounds.left.into(), bounds.top.into()),
                size: size(
                    (bounds.right - bounds.left).into(),
                    (bounds.bottom - bounds.top).into(),
                ),
            })
        }
    }

    pub(super) fn glyph_for_char(&self, font_id: FontId, ch: char) -> Option<GlyphId> {
        let font_info = &self.fonts[font_id.0];
        let codepoints = ch as u32;
        let mut glyph_indices = 0u16;
        unsafe {
            font_info
                .font_face
                .GetGlyphIndices(&raw const codepoints, 1, &raw mut glyph_indices)
                .log_err()
        }
        .map(|_| GlyphId(glyph_indices as u32))
    }

    pub(super) fn rasterize_glyph(
        &self,
        components: &DirectWriteComponents,
        params: &RenderGlyphParams,
        glyph_bounds: Bounds<DevicePixels>,
    ) -> Result<(Size<DevicePixels>, Vec<u8>)> {
        if glyph_bounds.size.width.0 == 0 || glyph_bounds.size.height.0 == 0 {
            anyhow::bail!("glyph bounds are empty");
        }

        let bitmap_data = if params.is_emoji {
            if let Ok(color) = self.rasterize_color(components, params, glyph_bounds) {
                color
            } else {
                let monochrome = self.rasterize_monochrome(components, params, glyph_bounds)?;
                monochrome
                    .into_iter()
                    .flat_map(|pixel| [0, 0, 0, pixel])
                    .collect::<Vec<_>>()
            }
        } else {
            self.rasterize_monochrome(components, params, glyph_bounds)?
        };

        Ok((glyph_bounds.size, bitmap_data))
    }

    pub(super) fn rasterize_monochrome(
        &self,
        components: &DirectWriteComponents,
        params: &RenderGlyphParams,
        glyph_bounds: Bounds<DevicePixels>,
    ) -> Result<Vec<u8>> {
        let glyph_analysis = self.create_glyph_run_analysis(components, params)?;
        if !params.subpixel_rendering {
            let mut bitmap_data =
                vec![0u8; glyph_bounds.size.width.0 as usize * glyph_bounds.size.height.0 as usize];
            unsafe {
                glyph_analysis.CreateAlphaTexture(
                    DWRITE_TEXTURE_ALIASED_1x1,
                    &RECT {
                        left: glyph_bounds.origin.x.0,
                        top: glyph_bounds.origin.y.0,
                        right: glyph_bounds.size.width.0 + glyph_bounds.origin.x.0,
                        bottom: glyph_bounds.size.height.0 + glyph_bounds.origin.y.0,
                    },
                    &mut bitmap_data,
                )?;
            }

            return Ok(bitmap_data);
        }

        let width = glyph_bounds.size.width.0 as usize;
        let height = glyph_bounds.size.height.0 as usize;
        let pixel_count = width * height;

        let mut bitmap_data = vec![0u8; pixel_count * 4];

        unsafe {
            glyph_analysis.CreateAlphaTexture(
                DWRITE_TEXTURE_CLEARTYPE_3x1,
                &RECT {
                    left: glyph_bounds.origin.x.0,
                    top: glyph_bounds.origin.y.0,
                    right: glyph_bounds.size.width.0 + glyph_bounds.origin.x.0,
                    bottom: glyph_bounds.size.height.0 + glyph_bounds.origin.y.0,
                },
                &mut bitmap_data[..pixel_count * 3],
            )?;
        }

        // The output buffer expects RGBA data, so pad the alpha channel with zeros.
        for pixel_ix in (0..pixel_count).rev() {
            let src = pixel_ix * 3;
            let dst = pixel_ix * 4;
            (
                bitmap_data[dst],
                bitmap_data[dst + 1],
                bitmap_data[dst + 2],
                bitmap_data[dst + 3],
            ) = (
                bitmap_data[src],
                bitmap_data[src + 1],
                bitmap_data[src + 2],
                0,
            );
        }

        Ok(bitmap_data)
    }
}
