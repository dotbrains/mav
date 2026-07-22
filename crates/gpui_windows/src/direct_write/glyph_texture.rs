use super::*;

pub(super) struct GlyphLayerTexture {
    pub(super) run_color: Rgba,
    pub(super) bounds: Bounds<i32>,
    pub(super) texture_view: ID3D11ShaderResourceView,
    // holding on to the texture to not RAII drop it
    pub(super) _texture: ID3D11Texture2D,
}

impl GlyphLayerTexture {
    pub(super) fn new(
        gpu_state: &GPUState,
        run_color: Rgba,
        bounds: Bounds<i32>,
        alpha_data: &[u8],
    ) -> Result<Self> {
        let texture_size = bounds.size;

        let desc = D3D11_TEXTURE2D_DESC {
            Width: texture_size.width as u32,
            Height: texture_size.height as u32,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_R8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
            CPUAccessFlags: 0,
            MiscFlags: 0,
        };

        let texture = {
            let mut texture: Option<ID3D11Texture2D> = None;
            unsafe {
                gpu_state
                    .device
                    .CreateTexture2D(&desc, None, Some(&mut texture))?
            };
            texture.unwrap()
        };
        let texture_view = {
            let mut view: Option<ID3D11ShaderResourceView> = None;
            unsafe {
                gpu_state
                    .device
                    .CreateShaderResourceView(&texture, None, Some(&mut view))?
            };
            view.unwrap()
        };

        unsafe {
            gpu_state.device_context.UpdateSubresource(
                &texture,
                0,
                None,
                alpha_data.as_ptr() as _,
                texture_size.width as u32,
                0,
            )
        };

        Ok(GlyphLayerTexture {
            run_color,
            bounds,
            texture_view,
            _texture: texture,
        })
    }
}

#[repr(C)]
pub(super) struct GlyphLayerTextureParams {
    pub(super) bounds: Bounds<i32>,
    pub(super) run_color: Rgba,
    pub(super) gamma_ratios: [f32; 4],
    pub(super) grayscale_enhanced_contrast: f32,
    pub(super) _pad: [f32; 3],
}
