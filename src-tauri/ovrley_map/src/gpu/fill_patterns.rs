//! The `fill-pattern` images, bundled so that drawing them needs no download. See
//! `patterns/NOTICE.md`.
//!
//! Each image has its own repeating, mipmapped texture rather than a place in an atlas, because a
//! pitched view minifies them far enough to shimmer as the camera moves.

use wgpu::util::{DeviceExt, TextureDataOrder};

/// Every bundled image is @2x, so a texel is half a style pixel.
struct PatternImage {
    name: &'static str,
    size: u32,
    rgba: &'static [u8],
}

#[cfg(target_arch = "wasm32")]
pub(crate) const PATTERN_COUNT: usize = IMAGES.len();

const IMAGES: [PatternImage; 2] = [
    PatternImage {
        name: "wetland_bg_11",
        size: 30,
        rgba: include_bytes!("patterns/wetland_bg_11.rgba"),
    },
    PatternImage {
        name: "pedestrian_polygon",
        size: 128,
        rgba: include_bytes!("patterns/pedestrian_polygon.rgba"),
    },
];

/// Style pixels after which every image repeats: the lowest common multiple of 15 and 64. Tile
/// origins wrap at this, because a whole-world pixel position doesn't fit in the shader's `f32`.
pub(crate) const PATTERN_PERIOD_PX: f64 = 960.0;

/// The bundled image of this name.
pub(crate) fn pattern_index(name: &str) -> Option<u8> {
    IMAGES
        .iter()
        .position(|image| image.name == name)
        .map(|index| index as u8)
}

/// One bind group per bundled image, in the order of [`pattern_index`].
pub(crate) fn pattern_bind_groups(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layout: &wgpu::BindGroupLayout,
) -> Vec<wgpu::BindGroup> {
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("fill pattern sampler"),
        address_mode_u: wgpu::AddressMode::Repeat,
        address_mode_v: wgpu::AddressMode::Repeat,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Linear,
        ..Default::default()
    });
    IMAGES
        .iter()
        .map(|image| {
            let (mip_level_count, texels) = premultiplied_mips(image);
            let texture = device.create_texture_with_data(
                queue,
                &wgpu::TextureDescriptor {
                    label: Some(image.name),
                    size: wgpu::Extent3d {
                        width: image.size,
                        height: image.size,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[],
                },
                TextureDataOrder::LayerMajor,
                &texels,
            );
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(image.name),
                layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                ],
            })
        })
        .collect()
}

/// Every mip of a premultiplied image, largest first. Premultiplied so that filtering doesn't pull
/// the colour of transparent texels into the pattern's edges.
fn premultiplied_mips(image: &PatternImage) -> (u32, Vec<u8>) {
    let mut level: Vec<[f32; 4]> = image
        .rgba
        .chunks_exact(4)
        .map(|texel| {
            let alpha = f32::from(texel[3]) / 255.0;
            [
                f32::from(texel[0]) * alpha,
                f32::from(texel[1]) * alpha,
                f32::from(texel[2]) * alpha,
                f32::from(texel[3]),
            ]
        })
        .collect();
    let mut size = image.size as usize;
    let mut levels = 1;
    let mut bytes = Vec::with_capacity(image.rgba.len() * 2);
    loop {
        bytes.extend(
            level
                .iter()
                .flatten()
                .map(|channel| channel.round().clamp(0.0, 255.0) as u8),
        );
        if size == 1 {
            return (levels, bytes);
        }
        let next = size / 2;
        level = halved(&level, size, next);
        size = next;
        levels += 1;
    }
}

/// A box filter down to `next` texels a side. Odd sizes lose nothing: each new texel averages the
/// source texels its area touches, wrapping because these images repeat.
fn halved(level: &[[f32; 4]], size: usize, next: usize) -> Vec<[f32; 4]> {
    let span = |index: usize| {
        let start = index * size / next;
        let end = ((index + 1) * size).div_ceil(next);
        start..end
    };
    let mut halved = Vec::with_capacity(next * next);
    for y in 0..next {
        for x in 0..next {
            let mut sum = [0.0; 4];
            let mut count = 0.0;
            for source_y in span(y) {
                for source_x in span(x) {
                    let texel = level[source_y % size * size + source_x % size];
                    for (total, channel) in sum.iter_mut().zip(texel) {
                        *total += channel;
                    }
                    count += 1.0;
                }
            }
            halved.push(sum.map(|total| total / count));
        }
    }
    halved
}
