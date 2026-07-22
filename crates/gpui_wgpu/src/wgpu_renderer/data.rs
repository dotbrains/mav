use super::*;
use bytemuck::{Pod, Zeroable};
use gpui::{Background, Bounds, ContentMask, DevicePixels, Point, ScaledPixels, Size};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct GlobalParams {
    pub(super) viewport_size: [f32; 2],
    pub(super) premultiplied_alpha: u32,
    pub(super) pad: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct PodBounds {
    pub(super) origin: [f32; 2],
    pub(super) size: [f32; 2],
}

impl From<Bounds<ScaledPixels>> for PodBounds {
    fn from(bounds: Bounds<ScaledPixels>) -> Self {
        Self {
            origin: [bounds.origin.x.0, bounds.origin.y.0],
            size: [bounds.size.width.0, bounds.size.height.0],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct PodContentMask {
    pub(super) bounds: PodBounds,
    pub(super) corner_radii: [f32; 4],
}

impl From<ContentMask<ScaledPixels>> for PodContentMask {
    fn from(content_mask: ContentMask<ScaledPixels>) -> Self {
        Self {
            bounds: content_mask.bounds.into(),
            corner_radii: [
                content_mask.corner_radii.top_left.0,
                content_mask.corner_radii.top_right.0,
                content_mask.corner_radii.bottom_right.0,
                content_mask.corner_radii.bottom_left.0,
            ],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct SurfaceParams {
    pub(super) bounds: PodBounds,
    pub(super) content_mask: PodContentMask,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct GammaParams {
    pub(super) gamma_ratios: [f32; 4],
    pub(super) grayscale_enhanced_contrast: f32,
    pub(super) subpixel_enhanced_contrast: f32,
    pub(super) is_bgr: u32,
    pub(super) _pad: u32,
}

#[derive(Clone, Debug)]
#[repr(C)]
pub(super) struct PathSprite {
    pub(super) bounds: Bounds<ScaledPixels>,
}

#[derive(Clone, Debug)]
#[repr(C)]
pub(super) struct PathRasterizationVertex {
    pub(super) xy_position: Point<ScaledPixels>,
    pub(super) st_position: Point<f32>,
    pub(super) color: Background,
    pub(super) bounds: Bounds<ScaledPixels>,
}

pub struct WgpuSurfaceConfig {
    pub size: Size<DevicePixels>,
    pub transparent: bool,
    /// Preferred presentation mode. When `Some`, the renderer will use this
    /// mode if supported by the surface, falling back to `Fifo`.
    /// When `None`, defaults to `Fifo` (VSync).
    ///
    /// Mobile platforms may prefer `Mailbox` (triple-buffering) to avoid
    /// blocking in `get_current_texture()` during lifecycle transitions.
    pub preferred_present_mode: Option<wgpu::PresentMode>,
}

pub(super) struct WgpuPipelines {
    pub(super) quads: wgpu::RenderPipeline,
    pub(super) shadows: wgpu::RenderPipeline,
    pub(super) path_rasterization: wgpu::RenderPipeline,
    pub(super) paths: wgpu::RenderPipeline,
    pub(super) underlines: wgpu::RenderPipeline,
    pub(super) mono_sprites: wgpu::RenderPipeline,
    pub(super) subpixel_sprites: Option<wgpu::RenderPipeline>,
    pub(super) poly_sprites: wgpu::RenderPipeline,
    #[allow(dead_code)]
    pub(super) surfaces: wgpu::RenderPipeline,
}

pub(super) struct WgpuBindGroupLayouts {
    pub(super) globals: wgpu::BindGroupLayout,
    pub(super) instances: wgpu::BindGroupLayout,
    pub(super) instances_with_texture: wgpu::BindGroupLayout,
    pub(super) surfaces: wgpu::BindGroupLayout,
}

/// Shared GPU context reference, used to coordinate device recovery across multiple windows.
pub type GpuContext = Rc<RefCell<Option<WgpuContext>>>;

/// GPU resources that must be dropped together during device recovery.
pub(super) struct WgpuResources {
    pub(super) device: Arc<wgpu::Device>,
    pub(super) queue: Arc<wgpu::Queue>,
    pub(super) surface: wgpu::Surface<'static>,
    pub(super) pipelines: WgpuPipelines,
    pub(super) bind_group_layouts: WgpuBindGroupLayouts,
    pub(super) atlas_sampler: wgpu::Sampler,
    pub(super) globals_buffer: wgpu::Buffer,
    pub(super) globals_bind_group: wgpu::BindGroup,
    pub(super) path_globals_bind_group: wgpu::BindGroup,
    pub(super) instance_buffer: wgpu::Buffer,
    pub(super) path_intermediate_texture: Option<wgpu::Texture>,
    pub(super) path_intermediate_view: Option<wgpu::TextureView>,
    pub(super) path_msaa_texture: Option<wgpu::Texture>,
    pub(super) path_msaa_view: Option<wgpu::TextureView>,
}

impl WgpuResources {
    pub(super) fn invalidate_intermediate_textures(&mut self) {
        self.path_intermediate_texture = None;
        self.path_intermediate_view = None;
        self.path_msaa_texture = None;
        self.path_msaa_view = None;
    }
}
