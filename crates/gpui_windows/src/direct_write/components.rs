use super::*;

pub(super) struct FontInfo {
    pub(super) font_family_h: HSTRING,
    pub(super) font_face: IDWriteFontFace3,
    pub(super) features: IDWriteTypography,
    pub(super) fallbacks: Option<IDWriteFontFallback>,
    pub(super) font_collection: IDWriteFontCollection1,
}

pub(crate) struct DirectWriteTextSystem {
    pub(super) components: DirectWriteComponents,
    pub(super) state: RwLock<DirectWriteState>,
}

pub(super) struct DirectWriteComponents {
    pub(super) locale: HSTRING,
    pub(super) factory: IDWriteFactory5,
    pub(super) in_memory_loader: IDWriteInMemoryFontFileLoader,
    pub(super) builder: IDWriteFontSetBuilder1,
    pub(super) text_renderer: TextRendererWrapper,
    pub(super) system_ui_font_name: SharedString,
    pub(super) system_subpixel_rendering: bool,
}

impl Drop for DirectWriteComponents {
    fn drop(&mut self) {
        unsafe {
            let _ = self
                .factory
                .UnregisterFontFileLoader(&self.in_memory_loader);
        }
    }
}

pub(super) struct GPUState {
    pub(super) device: ID3D11Device,
    pub(super) device_context: ID3D11DeviceContext,
    pub(super) sampler: Option<ID3D11SamplerState>,
    pub(super) blend_state: ID3D11BlendState,
    pub(super) vertex_shader: ID3D11VertexShader,
    pub(super) pixel_shader: ID3D11PixelShader,
}

pub(super) struct DirectWriteState {
    pub(super) gpu_state: GPUState,
    pub(super) system_font_collection: IDWriteFontCollection1,
    pub(super) custom_font_collection: IDWriteFontCollection1,
    pub(super) fonts: Vec<FontInfo>,
    pub(super) font_to_font_id: HashMap<Font, FontId>,
    pub(super) font_info_cache: HashMap<usize, FontId>,
    pub(super) layout_line_scratch: Vec<u16>,
}

impl GPUState {
    pub(super) fn new(directx_devices: &DirectXDevices) -> Result<Self> {
        let device = directx_devices.device.clone();
        let device_context = directx_devices.device_context.clone();

        let blend_state = {
            let mut blend_state = None;
            let desc = D3D11_BLEND_DESC {
                AlphaToCoverageEnable: false.into(),
                IndependentBlendEnable: false.into(),
                RenderTarget: [
                    D3D11_RENDER_TARGET_BLEND_DESC {
                        BlendEnable: true.into(),
                        SrcBlend: D3D11_BLEND_ONE,
                        DestBlend: D3D11_BLEND_INV_SRC_ALPHA,
                        BlendOp: D3D11_BLEND_OP_ADD,
                        SrcBlendAlpha: D3D11_BLEND_ONE,
                        DestBlendAlpha: D3D11_BLEND_INV_SRC_ALPHA,
                        BlendOpAlpha: D3D11_BLEND_OP_ADD,
                        RenderTargetWriteMask: D3D11_COLOR_WRITE_ENABLE_ALL.0 as u8,
                    },
                    Default::default(),
                    Default::default(),
                    Default::default(),
                    Default::default(),
                    Default::default(),
                    Default::default(),
                    Default::default(),
                ],
            };
            unsafe { device.CreateBlendState(&desc, Some(&mut blend_state)) }?;
            blend_state.unwrap()
        };

        let sampler = {
            let mut sampler = None;
            let desc = D3D11_SAMPLER_DESC {
                Filter: D3D11_FILTER_MIN_MAG_MIP_POINT,
                AddressU: D3D11_TEXTURE_ADDRESS_BORDER,
                AddressV: D3D11_TEXTURE_ADDRESS_BORDER,
                AddressW: D3D11_TEXTURE_ADDRESS_BORDER,
                MipLODBias: 0.0,
                MaxAnisotropy: 1,
                ComparisonFunc: D3D11_COMPARISON_ALWAYS,
                BorderColor: [0.0, 0.0, 0.0, 0.0],
                MinLOD: 0.0,
                MaxLOD: 0.0,
            };
            unsafe { device.CreateSamplerState(&desc, Some(&mut sampler)) }?;
            sampler
        };

        let vertex_shader = {
            let source = shader_resources::RawShaderBytes::new(
                shader_resources::ShaderModule::EmojiRasterization,
                shader_resources::ShaderTarget::Vertex,
            )?;
            let mut shader = None;
            unsafe { device.CreateVertexShader(source.as_bytes(), None, Some(&mut shader)) }?;
            shader.unwrap()
        };

        let pixel_shader = {
            let source = shader_resources::RawShaderBytes::new(
                shader_resources::ShaderModule::EmojiRasterization,
                shader_resources::ShaderTarget::Fragment,
            )?;
            let mut shader = None;
            unsafe { device.CreatePixelShader(source.as_bytes(), None, Some(&mut shader)) }?;
            shader.unwrap()
        };

        Ok(Self {
            device,
            device_context,
            sampler,
            blend_state,
            vertex_shader,
            pixel_shader,
        })
    }
}
