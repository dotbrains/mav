
use anyhow::Result;

#[cfg(debug_assertions)]
use windows::{
    Win32::Graphics::Direct3D::{
        Fxc::{D3DCOMPILE_DEBUG, D3DCOMPILE_SKIP_OPTIMIZATION, D3DCompileFromFile},
        ID3DBlob,
    },
    core::{HSTRING, PCSTR},
};

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) enum ShaderModule {
    Quad,
    Shadow,
    Underline,
    PathRasterization,
    PathSprite,
    MonochromeSprite,
    SubpixelSprite,
    PolychromeSprite,
    EmojiRasterization,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) enum ShaderTarget {
    Vertex,
    Fragment,
}

pub(crate) struct RawShaderBytes<'t> {
    inner: &'t [u8],

    #[cfg(debug_assertions)]
    _blob: ID3DBlob,
}

impl<'t> RawShaderBytes<'t> {
    pub(crate) fn new(module: ShaderModule, target: ShaderTarget) -> Result<Self> {
        #[cfg(not(debug_assertions))]
        {
            Ok(Self::from_bytes(module, target))
        }
        #[cfg(debug_assertions)]
        {
            let blob = build_shader_blob(module, target)?;
            let inner = unsafe {
                std::slice::from_raw_parts(
                    blob.GetBufferPointer() as *const u8,
                    blob.GetBufferSize(),
                )
            };
            Ok(Self { inner, _blob: blob })
        }
    }

    pub(crate) fn as_bytes(&'t self) -> &'t [u8] {
        self.inner
    }

    #[cfg(not(debug_assertions))]
    fn from_bytes(module: ShaderModule, target: ShaderTarget) -> Self {
        let bytes = match module {
            ShaderModule::Quad => match target {
                ShaderTarget::Vertex => QUAD_VERTEX_BYTES,
                ShaderTarget::Fragment => QUAD_FRAGMENT_BYTES,
            },
            ShaderModule::Shadow => match target {
                ShaderTarget::Vertex => SHADOW_VERTEX_BYTES,
                ShaderTarget::Fragment => SHADOW_FRAGMENT_BYTES,
            },
            ShaderModule::Underline => match target {
                ShaderTarget::Vertex => UNDERLINE_VERTEX_BYTES,
                ShaderTarget::Fragment => UNDERLINE_FRAGMENT_BYTES,
            },
            ShaderModule::PathRasterization => match target {
                ShaderTarget::Vertex => PATH_RASTERIZATION_VERTEX_BYTES,
                ShaderTarget::Fragment => PATH_RASTERIZATION_FRAGMENT_BYTES,
            },
            ShaderModule::PathSprite => match target {
                ShaderTarget::Vertex => PATH_SPRITE_VERTEX_BYTES,
                ShaderTarget::Fragment => PATH_SPRITE_FRAGMENT_BYTES,
            },
            ShaderModule::MonochromeSprite => match target {
                ShaderTarget::Vertex => MONOCHROME_SPRITE_VERTEX_BYTES,
                ShaderTarget::Fragment => MONOCHROME_SPRITE_FRAGMENT_BYTES,
            },
            ShaderModule::SubpixelSprite => match target {
                ShaderTarget::Vertex => SUBPIXEL_SPRITE_VERTEX_BYTES,
                ShaderTarget::Fragment => SUBPIXEL_SPRITE_FRAGMENT_BYTES,
            },
            ShaderModule::PolychromeSprite => match target {
                ShaderTarget::Vertex => POLYCHROME_SPRITE_VERTEX_BYTES,
                ShaderTarget::Fragment => POLYCHROME_SPRITE_FRAGMENT_BYTES,
            },
            ShaderModule::EmojiRasterization => match target {
                ShaderTarget::Vertex => EMOJI_RASTERIZATION_VERTEX_BYTES,
                ShaderTarget::Fragment => EMOJI_RASTERIZATION_FRAGMENT_BYTES,
            },
        };
        Self { inner: bytes }
    }
}

#[cfg(debug_assertions)]
pub(super) fn build_shader_blob(entry: ShaderModule, target: ShaderTarget) -> Result<ID3DBlob> {
    unsafe {
        use windows::Win32::Graphics::{
            Direct3D::ID3DInclude, Hlsl::D3D_COMPILE_STANDARD_FILE_INCLUDE,
        };

        let shader_name = if matches!(entry, ShaderModule::EmojiRasterization) {
            "color_text_raster.hlsl"
        } else {
            "shaders.hlsl"
        };

        let entry = format!(
            "{}_{}\0",
            entry.as_str(),
            match target {
                ShaderTarget::Vertex => "vertex",
                ShaderTarget::Fragment => "fragment",
            }
        );
        let target = match target {
            ShaderTarget::Vertex => "vs_4_1\0",
            ShaderTarget::Fragment => "ps_4_1\0",
        };

        let mut compile_blob = None;
        let mut error_blob = None;
        let shader_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(&format!("src/{}", shader_name))
            .canonicalize()?;

        let entry_point = PCSTR::from_raw(entry.as_ptr());
        let target_cstr = PCSTR::from_raw(target.as_ptr());

        // really dirty trick because winapi bindings are unhappy otherwise
        let include_handler =
            &std::mem::transmute::<usize, ID3DInclude>(D3D_COMPILE_STANDARD_FILE_INCLUDE as usize);

        let ret = D3DCompileFromFile(
            &HSTRING::from(shader_path.to_str().unwrap()),
            None,
            include_handler,
            entry_point,
            target_cstr,
            D3DCOMPILE_DEBUG | D3DCOMPILE_SKIP_OPTIMIZATION,
            0,
            &mut compile_blob,
            Some(&mut error_blob),
        );
        if ret.is_err() {
            let Some(error_blob) = error_blob else {
                return Err(anyhow::anyhow!("{ret:?}"));
            };

            let error_string = std::ffi::CStr::from_ptr(error_blob.GetBufferPointer() as *const i8)
                .to_string_lossy();
            log::error!("Shader compile error: {}", error_string);
            return Err(anyhow::anyhow!("Compile error: {}", error_string));
        }
        Ok(compile_blob.unwrap())
    }
}

#[cfg(not(debug_assertions))]
include!(concat!(env!("OUT_DIR"), "/shaders_bytes.rs"));

#[cfg(debug_assertions)]
impl ShaderModule {
    pub fn as_str(self) -> &'static str {
        match self {
            ShaderModule::Quad => "quad",
            ShaderModule::Shadow => "shadow",
            ShaderModule::Underline => "underline",
            ShaderModule::PathRasterization => "path_rasterization",
            ShaderModule::PathSprite => "path_sprite",
            ShaderModule::MonochromeSprite => "monochrome_sprite",
            ShaderModule::SubpixelSprite => "subpixel_sprite",
            ShaderModule::PolychromeSprite => "polychrome_sprite",
            ShaderModule::EmojiRasterization => "emoji_rasterization",
        }
    }
}
