use super::*;

impl DirectWriteState {
    pub(super) fn select_and_cache_font(
        &mut self,
        components: &DirectWriteComponents,
        font: &Font,
    ) -> Option<FontId> {
        let select_font = |this: &mut DirectWriteState, font: &Font| -> Option<FontId> {
            let info = [&this.custom_font_collection, &this.system_font_collection]
                .into_iter()
                .find_map(|font_collection| unsafe {
                    DirectWriteState::make_font_from_font_collection(
                        font,
                        font_collection,
                        &components.factory,
                        &this.system_font_collection,
                        &components.system_ui_font_name,
                    )
                })?;

            let font_id = FontId(this.fonts.len());
            let font_face_key = info.font_face.cast::<IUnknown>().unwrap().as_raw().addr();
            this.fonts.push(info);
            this.font_info_cache.insert(font_face_key, font_id);
            Some(font_id)
        };

        let mut font_id = select_font(self, font);
        if font_id.is_none() {
            // try updating system fonts and reselect
            let mut collection = None;
            let font_collection_updated = unsafe {
                components
                    .factory
                    .GetSystemFontCollection(false, &mut collection, true)
            }
            .log_err()
            .is_some();
            if font_collection_updated && let Some(collection) = collection {
                self.system_font_collection = collection;
            }
            font_id = select_font(self, font);
        };
        let font_id = font_id?;
        self.font_to_font_id.insert(font.clone(), font_id);
        Some(font_id)
    }
}

pub(super) fn add_fonts(
    &mut self,
    components: &DirectWriteComponents,
    fonts: Vec<Cow<'static, [u8]>>,
) -> Result<()> {
    for font_data in fonts {
        match font_data {
            Cow::Borrowed(data) => unsafe {
                let font_file = components
                    .in_memory_loader
                    .CreateInMemoryFontFileReference(
                        &components.factory,
                        data.as_ptr().cast(),
                        data.len() as _,
                        None,
                    )?;
                components.builder.AddFontFile(&font_file)?;
            },
            Cow::Owned(data) => unsafe {
                let font_file = components
                    .in_memory_loader
                    .CreateInMemoryFontFileReference(
                        &components.factory,
                        data.as_ptr().cast(),
                        data.len() as _,
                        None,
                    )?;
                components.builder.AddFontFile(&font_file)?;
            },
        }
    }
    let set = unsafe { components.builder.CreateFontSet()? };
    let collection = unsafe { components.factory.CreateFontCollectionFromFontSet(&set)? };
    self.custom_font_collection = collection;

    Ok(())
}

pub(super) fn generate_font_fallbacks(
    fallbacks: &FontFallbacks,
    factory: &IDWriteFactory5,
    system_font_collection: &IDWriteFontCollection1,
) -> Result<Option<IDWriteFontFallback>> {
    let fallback_list = fallbacks.fallback_list();
    if fallback_list.is_empty() {
        return Ok(None);
    }
    unsafe {
        let builder = factory.CreateFontFallbackBuilder()?;
        let font_set = &system_font_collection.GetFontSet()?;
        let mut unicode_ranges = Vec::new();
        for family_name in fallback_list {
            let family_name = HSTRING::from(family_name);
            let Some(fonts) = font_set
                .GetMatchingFonts(
                    &family_name,
                    DWRITE_FONT_WEIGHT_NORMAL,
                    DWRITE_FONT_STRETCH_NORMAL,
                    DWRITE_FONT_STYLE_NORMAL,
                )
                .log_err()
            else {
                continue;
            };
            let Ok(font_face) = fonts.GetFontFaceReference(0) else {
                continue;
            };
            let font = font_face.CreateFontFace()?;
            let mut count = 0;
            font.GetUnicodeRanges(None, &mut count).ok();
            if count == 0 {
                continue;
            }
            unicode_ranges.clear();
            unicode_ranges.resize_with(count as usize, DWRITE_UNICODE_RANGE::default);
            let Some(_) = font
                .GetUnicodeRanges(Some(&mut unicode_ranges), &mut count)
                .log_err()
            else {
                continue;
            };
            builder.AddMapping(
                &unicode_ranges,
                &[family_name.as_ptr()],
                None,
                None,
                None,
                1.0,
            )?;
        }
        let system_fallbacks = factory.GetSystemFontFallback()?;
        builder.AddMappings(&system_fallbacks)?;
        Ok(Some(builder.CreateFontFallback()?))
    }
}

unsafe fn generate_font_features(
    factory: &IDWriteFactory5,
    font_features: &FontFeatures,
) -> Result<IDWriteTypography> {
    let direct_write_features = unsafe { factory.CreateTypography()? };
    apply_font_features(&direct_write_features, font_features)?;
    Ok(direct_write_features)
}

unsafe fn make_font_from_font_collection(
    &Font {
        ref family,
        ref features,
        ref fallbacks,
        weight,
        style,
    }: &Font,
    collection: &IDWriteFontCollection1,
    factory: &IDWriteFactory5,
    system_font_collection: &IDWriteFontCollection1,
    system_ui_font_name: &SharedString,
) -> Option<FontInfo> {
    const SYSTEM_UI_FONT_NAME: &str = ".SystemUIFont";
    let family = if family == SYSTEM_UI_FONT_NAME {
        system_ui_font_name
    } else {
        gpui::font_name_with_fallbacks_shared(&family, &system_ui_font_name)
    };
    let fontset = unsafe { collection.GetFontSet().log_err()? };
    let font_family_h = HSTRING::from(family.as_str());
    let font = unsafe {
        fontset
            .GetMatchingFonts(
                &font_family_h,
                font_weight_to_dwrite(weight),
                DWRITE_FONT_STRETCH_NORMAL,
                font_style_to_dwrite(style),
            )
            .log_err()?
    };
    let total_number = unsafe { font.GetFontCount() };
    for index in 0..total_number {
        let res = maybe!({
            let font_face_ref = unsafe { font.GetFontFaceReference(index).log_err()? };
            let font_face = unsafe { font_face_ref.CreateFontFace().log_err()? };
            let direct_write_features =
                unsafe { Self::generate_font_features(factory, features).log_err()? };
            let fallbacks = fallbacks.as_ref().and_then(|fallbacks| {
                Self::generate_font_fallbacks(fallbacks, factory, system_font_collection)
                    .log_err()
                    .flatten()
            });
            let font_info = FontInfo {
                font_family_h: font_family_h.clone(),
                font_face,
                features: direct_write_features,
                fallbacks,
                font_collection: collection.clone(),
            };
            Some(font_info)
        });
        if res.is_some() {
            return res;
        }
    }
    None
}
