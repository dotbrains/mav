use super::*;
use gpui::block_on;
use gpui::{ImageId, RenderImageParams};
use std::sync::Arc;

fn test_device_and_queue() -> anyhow::Result<(Arc<wgpu::Device>, Arc<wgpu::Queue>)> {
    block_on(async {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            flags: wgpu::InstanceFlags::default(),
            backend_options: wgpu::BackendOptions::default(),
            memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
            display: None,
        });
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                compatible_surface: None,
                force_fallback_adapter: false,
            })
            .await
            .map_err(|error| anyhow::anyhow!("failed to request adapter: {error}"))?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("wgpu_atlas_test_device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::downlevel_defaults()
                    .using_resolution(adapter.limits())
                    .using_alignment(adapter.limits()),
                memory_hints: wgpu::MemoryHints::MemoryUsage,
                trace: wgpu::Trace::Off,
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
            })
            .await
            .map_err(|error| anyhow::anyhow!("failed to request device: {error}"))?;
        Ok((Arc::new(device), Arc::new(queue)))
    })
}

#[test]
fn before_frame_skips_uploads_for_removed_texture() -> anyhow::Result<()> {
    let (device, queue) = test_device_and_queue()?;

    let atlas = WgpuAtlas::new(device, queue, wgpu::TextureFormat::Bgra8Unorm);
    let key = AtlasKey::Image(RenderImageParams {
        image_id: ImageId(1),
        frame_index: 0,
    });
    let size = Size {
        width: DevicePixels(1),
        height: DevicePixels(1),
    };
    let mut build = || Ok(Some((size, Cow::Owned(vec![0, 0, 0, 255]))));

    // Regression test: before the fix, this panicked in flush_uploads
    atlas
        .get_or_insert_with(&key, &mut build)?
        .expect("tile should be created");
    atlas.remove(&key);
    atlas.before_frame();
    Ok(())
}

#[test]
fn remove_deallocates_tile_space_for_reuse() -> anyhow::Result<()> {
    let (device, queue) = test_device_and_queue()?;
    let atlas = WgpuAtlas::new(device, queue, wgpu::TextureFormat::Bgra8Unorm);

    let small = Size {
        width: DevicePixels(64),
        height: DevicePixels(64),
    };
    let big = Size {
        width: DevicePixels(700),
        height: DevicePixels(700),
    };

    let make_key = |image_id: usize| {
        AtlasKey::Image(RenderImageParams {
            image_id: ImageId(image_id),
            frame_index: 0,
        })
    };
    let insert = |key: &AtlasKey, size: Size<DevicePixels>| {
        let byte_count = (size.width.0 as usize) * (size.height.0 as usize) * 4;
        atlas
            .get_or_insert_with(key, &mut || {
                Ok(Some((size, Cow::Owned(vec![0u8; byte_count]))))
            })
            .expect("allocation should succeed")
            .expect("callback returns Some")
    };

    let keeper_key = make_key(1);
    let big_key_a = make_key(2);
    let big_key_b = make_key(3);

    let keeper_tile = insert(&keeper_key, small);
    let tile_a = insert(&big_key_a, big);
    assert_eq!(keeper_tile.texture_id, tile_a.texture_id);

    atlas.remove(&big_key_a);
    let tile_b = insert(&big_key_b, big);
    assert_eq!(tile_b.texture_id, keeper_tile.texture_id);
    Ok(())
}

#[test]
fn swizzle_upload_data_preserves_bgra_uploads() {
    let input = vec![0x10, 0x20, 0x30, 0x40];
    assert_eq!(
        swizzle_upload_data(&input, wgpu::TextureFormat::Bgra8Unorm),
        input
    );
}

#[test]
fn swizzle_upload_data_converts_bgra_to_rgba() {
    let input = vec![0x10, 0x20, 0x30, 0x40, 0xAA, 0xBB, 0xCC, 0xDD];
    assert_eq!(
        swizzle_upload_data(&input, wgpu::TextureFormat::Rgba8Unorm),
        vec![0x30, 0x20, 0x10, 0x40, 0xCC, 0xBB, 0xAA, 0xDD]
    );
}
