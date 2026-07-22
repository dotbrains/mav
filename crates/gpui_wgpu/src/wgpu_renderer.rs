mod construction;
mod data;
mod draw;
mod lifecycle;
mod pipelines;
mod primitives;
mod rendering_parameters;
mod surface;

use crate::{CompositorGpuHint, WgpuAtlas, WgpuContext};
use data::{WgpuBindGroupLayouts, WgpuPipelines, WgpuResources};
use gpui::{DevicePixels, GpuSpecs, Size};
#[cfg(not(target_family = "wasm"))]
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use rendering_parameters::RenderingParameters;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

pub use data::{GpuContext, WgpuSurfaceConfig};

pub struct WgpuRenderer {
    /// Shared GPU context for device recovery coordination (unused on WASM).
    #[allow(dead_code)]
    context: Option<GpuContext>,
    /// Compositor GPU hint for adapter selection (unused on WASM).
    #[allow(dead_code)]
    compositor_gpu: Option<CompositorGpuHint>,
    resources: Option<WgpuResources>,
    surface_config: wgpu::SurfaceConfiguration,
    atlas: Arc<WgpuAtlas>,
    path_globals_offset: u64,
    gamma_offset: u64,
    instance_buffer_capacity: u64,
    max_buffer_size: u64,
    storage_buffer_alignment: u64,
    rendering_params: RenderingParameters,
    is_bgr: bool,
    dual_source_blending: bool,
    adapter_info: wgpu::AdapterInfo,
    transparent_alpha_mode: wgpu::CompositeAlphaMode,
    opaque_alpha_mode: wgpu::CompositeAlphaMode,
    max_texture_size: u32,
    last_error: Arc<Mutex<Option<String>>>,
    failed_frame_count: u32,
    device_lost: std::sync::Arc<std::sync::atomic::AtomicBool>,
    surface_configured: bool,
    needs_redraw: bool,
}

impl WgpuRenderer {
    fn resources(&self) -> &WgpuResources {
        self.resources
            .as_ref()
            .expect("GPU resources not available")
    }

    fn resources_mut(&mut self) -> &mut WgpuResources {
        self.resources
            .as_mut()
            .expect("GPU resources not available")
    }
}
