use super::*;

impl DirectWriteTextSystem {
    pub(crate) fn new(directx_devices: &DirectXDevices) -> Result<Self> {
        let factory: IDWriteFactory5 = unsafe { DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)? };
        // The `IDWriteInMemoryFontFileLoader` here is supported starting from
        // Windows 10 Creators Update, which consequently requires the entire
        // `DirectWriteTextSystem` to run on `win10 1703`+.
        let in_memory_loader = unsafe { factory.CreateInMemoryFontFileLoader()? };
        unsafe { factory.RegisterFontFileLoader(&in_memory_loader)? };
        let builder = unsafe { factory.CreateFontSetBuilder()? };
        let mut locale = [0u16; LOCALE_NAME_MAX_LENGTH as usize];
        unsafe { GetUserDefaultLocaleName(&mut locale) };
        let locale = HSTRING::from_wide(&locale);
        let text_renderer = TextRendererWrapper::new(locale.clone());

        let gpu_state = GPUState::new(directx_devices)?;

        let system_subpixel_rendering = get_system_subpixel_rendering();
        let system_ui_font_name = get_system_ui_font_name();
        let components = DirectWriteComponents {
            locale,
            factory,
            in_memory_loader,
            builder,
            text_renderer,
            system_ui_font_name,
            system_subpixel_rendering,
        };

        let system_font_collection = unsafe {
            let mut result = None;
            components
                .factory
                .GetSystemFontCollection(false, &mut result, true)?;
            result.context("Failed to get system font collection")?
        };
        let custom_font_set = unsafe { components.builder.CreateFontSet()? };
        let custom_font_collection = unsafe {
            components
                .factory
                .CreateFontCollectionFromFontSet(&custom_font_set)?
        };

        Ok(Self {
            components,
            state: RwLock::new(DirectWriteState {
                gpu_state,
                system_font_collection,
                custom_font_collection,
                fonts: Vec::new(),
                font_to_font_id: HashMap::default(),
                font_info_cache: HashMap::default(),
                layout_line_scratch: Vec::new(),
            }),
        })
    }

    pub(crate) fn handle_gpu_lost(&self, directx_devices: &DirectXDevices) -> Result<()> {
        self.state.write().handle_gpu_lost(directx_devices)
    }
}

impl PlatformTextSystem for DirectWriteTextSystem {
    fn add_fonts(&self, fonts: Vec<Cow<'static, [u8]>>) -> Result<()> {
        self.state.write().add_fonts(&self.components, fonts)
    }

    fn all_font_names(&self) -> Vec<String> {
        self.state.read().all_font_names(&self.components)
    }

    fn font_id(&self, font: &Font) -> Result<FontId> {
        let lock = self.state.upgradable_read();
        if let Some(font_id) = lock.font_to_font_id.get(font) {
            Ok(*font_id)
        } else {
            RwLockUpgradableReadGuard::upgrade(lock)
                .select_and_cache_font(&self.components, font)
                .with_context(|| format!("Failed to select font: {:?}", font))
        }
    }

    fn font_metrics(&self, font_id: FontId) -> FontMetrics {
        self.state.read().font_metrics(font_id)
    }

    fn typographic_bounds(&self, font_id: FontId, glyph_id: GlyphId) -> Result<Bounds<f32>> {
        self.state.read().get_typographic_bounds(font_id, glyph_id)
    }

    fn advance(&self, font_id: FontId, glyph_id: GlyphId) -> anyhow::Result<Size<f32>> {
        self.state.read().get_advance(font_id, glyph_id)
    }

    fn glyph_for_char(&self, font_id: FontId, ch: char) -> Option<GlyphId> {
        self.state.read().glyph_for_char(font_id, ch)
    }

    fn glyph_raster_bounds(
        &self,
        params: &RenderGlyphParams,
    ) -> anyhow::Result<Bounds<DevicePixels>> {
        self.state.read().raster_bounds(&self.components, params)
    }

    fn rasterize_glyph(
        &self,
        params: &RenderGlyphParams,
        raster_bounds: Bounds<DevicePixels>,
    ) -> anyhow::Result<(Size<DevicePixels>, Vec<u8>)> {
        self.state
            .read()
            .rasterize_glyph(&self.components, params, raster_bounds)
    }

    fn layout_line(&self, text: &str, font_size: Pixels, runs: &[FontRun]) -> LineLayout {
        self.state
            .write()
            .layout_line(&self.components, text, font_size, runs)
            .log_err()
            .unwrap_or(LineLayout {
                font_size,
                ..Default::default()
            })
    }

    fn recommended_rendering_mode(
        &self,
        _font_id: FontId,
        _font_size: Pixels,
    ) -> TextRenderingMode {
        if self.components.system_subpixel_rendering {
            TextRenderingMode::Subpixel
        } else {
            TextRenderingMode::Grayscale
        }
    }
}
