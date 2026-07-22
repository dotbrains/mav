mod color_raster;
mod components;
mod font_state;
mod glyph_texture;
mod layout;
mod raster;
mod system;
mod text_renderer;
mod utils;

#[cfg(test)]
mod tests;

pub(crate) use components::DirectWriteTextSystem;

pub(super) use std::{
    borrow::Cow,
    ffi::{c_uint, c_void},
    mem::ManuallyDrop,
};

pub(super) use anyhow::{Context, Result};
pub(super) use collections::HashMap;
pub(super) use gpui::*;
pub(super) use gpui_util::{ResultExt, maybe};
pub(super) use parking_lot::{RwLock, RwLockUpgradableReadGuard};
pub(super) use windows::{
    Win32::{
        Foundation::*,
        Globalization::GetUserDefaultLocaleName,
        Graphics::{
            Direct3D::D3D_PRIMITIVE_TOPOLOGY_TRIANGLESTRIP, Direct3D11::*, DirectWrite::*,
            Dxgi::Common::*, Gdi::LOGFONTW,
        },
        System::SystemServices::LOCALE_NAME_MAX_LENGTH,
        UI::WindowsAndMessaging::*,
    },
    core::*,
};
pub(super) use windows_numerics::Vector2;

pub(super) use crate::*;
