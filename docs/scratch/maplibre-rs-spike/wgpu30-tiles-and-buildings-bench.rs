//! Throwaway: GPU render + readback vs Skia CPU for a pitched map with tiles and buildings.
//! Same camera and geometry on both. Zurich's densest 500 m square: ~640 buildings, ~4060 walls.
use skia_safe as sk;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use wgpu::util::DeviceExt;

type M = [[f32; 4]; 4];
const TILE_PX: u32 = 1024;
const TILES_X: i32 = 5;
const TILES_Y: i32 = 4;
const TILE_M: f32 = 210.0;
const PITCH_DEG: f32 = 60.0;
const RANGE_M: f32 = 250.0;

fn mul(a: &M, b: &M) -> M {
    let mut r = [[0.0; 4]; 4];
    for c in 0..4 {
        for row in 0..4 {
            r[c][row] = (0..4).map(|k| a[k][row] * b[c][k]).sum();
        }
    }
    r
}
fn norm(v: [f32; 3]) -> [f32; 3] {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    [v[0] / l, v[1] / l, v[2] / l]
}
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
/// Clip from world, wgpu conventions, for a bearing around the focus at the origin.
fn clip_matrix(bearing_deg: f32) -> M {
    let fov = 36.87_f32.to_radians();
    let d = (RANGE_M / 2.0) / (fov / 2.0).tan();
    let p = PITCH_DEG.to_radians();
    let eye = [0.0, -d * p.sin(), d * p.cos()];
    let f = norm([-eye[0], -eye[1], -eye[2]]);
    let s = norm(cross(f, [0.0, 1.0, 0.0]));
    let u = cross(s, f);
    let view: M = [
        [s[0], u[0], -f[0], 0.0],
        [s[1], u[1], -f[1], 0.0],
        [s[2], u[2], -f[2], 0.0],
        [-dot(s, eye), -dot(u, eye), dot(f, eye), 1.0],
    ];
    let t = 1.0 / (fov / 2.0).tan();
    let (n, far) = (10.0, 4000.0);
    let proj: M = [
        [t, 0.0, 0.0, 0.0],
        [0.0, t, 0.0, 0.0],
        [0.0, 0.0, far / (n - far), -1.0],
        [0.0, 0.0, n * far / (n - far), 0.0],
    ];
    let b = bearing_deg.to_radians();
    let rot: M = [
        [b.cos(), b.sin(), 0.0, 0.0],
        [-b.sin(), b.cos(), 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];
    mul(&proj, &mul(&view, &rot))
}
fn bearing(frame: usize) -> f32 {
    (frame as f32 * 0.05).sin() * 10.0
}

struct Building {
    base: Vec<[f32; 2]>,
    height: f32,
}
fn buildings() -> Vec<Building> {
    let mut seed = 12345_u32;
    let mut rnd = move || {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        (seed >> 8) as f32 / 16777216.0
    };
    (0..640)
        .map(|i| {
            let (cx, cy) = (rnd() * 500.0 - 250.0, rnd() * 500.0 - 250.0);
            let radius = 6.0 + rnd() * 8.0;
            let height = 6.0 + rnd() * 24.0;
            let sides = if i % 5 == 0 { 8 } else { 6 };
            let base = (0..sides)
                .map(|k| {
                    let a = k as f32 / sides as f32 * std::f32::consts::TAU;
                    [cx + radius * a.cos(), cy + radius * a.sin()]
                })
                .collect();
            Building { base, height }
        })
        .collect()
}
fn shade(a: [f32; 2], b: [f32; 2]) -> f32 {
    let n = norm([b[1] - a[1], a[0] - b[0], 0.0]);
    0.72 + 0.2 * (n[0] * 0.5 + n[1] * 0.85)
}

fn tile_pixels() -> Vec<u8> {
    let mut px = vec![0_u8; (TILE_PX * TILE_PX * 4) as usize];
    for y in 0..TILE_PX {
        for x in 0..TILE_PX {
            let i = ((y * TILE_PX + x) * 4) as usize;
            let road = (x % 128) < 10 || (y % 170) < 8 || ((x + y) % 301) < 6;
            let v = if road { 255 } else { 225 + ((x ^ y) % 16) as u8 };
            px[i..i + 4].copy_from_slice(&[v, v, v - 8, 255]);
        }
    }
    px
}
fn tile_rect(tx: i32, ty: i32) -> [f32; 4] {
    let x0 = (tx - TILES_X / 2) as f32 * TILE_M - TILE_M / 2.0;
    let y0 = (ty - 1) as f32 * TILE_M - TILE_M / 2.0;
    [x0, y0, x0 + TILE_M, y0 + TILE_M]
}

const SHADER: &str = r#"
@group(0) @binding(0) var<uniform> m: mat4x4<f32>;
@group(1) @binding(0) var t: texture_2d<f32>;
@group(1) @binding(1) var s: sampler;
struct GV { @builtin(position) p: vec4f, @location(0) uv: vec2f };
@vertex fn vs_g(@location(0) pos: vec3f, @location(1) uv: vec2f) -> GV { return GV(m * vec4f(pos, 1.0), uv); }
@fragment fn fs_g(i: GV) -> @location(0) vec4f { return textureSample(t, s, i.uv); }
struct BV { @builtin(position) p: vec4f, @location(0) c: vec3f };
@vertex fn vs_b(@location(0) pos: vec3f, @location(1) c: vec3f) -> BV { return BV(m * vec4f(pos, 1.0), c); }
@fragment fn fs_b(i: BV) -> @location(0) vec4f { return vec4f(i.c, 1.0); }
"#;

struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    uniform: wgpu::Buffer,
    globals: wgpu::BindGroup,
    ground: wgpu::RenderPipeline,
    walls: wgpu::RenderPipeline,
    tiles: Vec<wgpu::BindGroup>,
    ground_vb: wgpu::Buffer,
    building_vb: wgpu::Buffer,
    building_verts: u32,
}

fn gpu_setup(pixels: &[u8]) -> Gpu {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        ..Default::default()
    }))
    .expect("adapter");
    let info = adapter.get_info();
    println!("GPU: {} ({:?}, {:?})", info.name, info.backend, info.device_type);
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).expect("device");

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(SHADER.into()),
    });
    let uniform = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    });
    let globals = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &globals_layout,
        entries: &[wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() }],
    });
    let tile_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Linear,
        anisotropy_clamp: 8,
        ..Default::default()
    });

    // Mip chain, built once on the CPU and shared by every synthetic tile.
    let mut mips = vec![(TILE_PX, pixels.to_vec())];
    while mips.last().unwrap().0 > 1 {
        let (size, src) = mips.last().unwrap();
        let (size, half) = (*size, *size / 2);
        let mut next = vec![0_u8; (half * half * 4) as usize];
        for y in 0..half {
            for x in 0..half {
                for c in 0..4 {
                    let at = |px: u32, py: u32| src[((py * size + px) * 4 + c) as usize] as u32;
                    let sum = at(x * 2, y * 2) + at(x * 2 + 1, y * 2) + at(x * 2, y * 2 + 1) + at(x * 2 + 1, y * 2 + 1);
                    next[((y * half + x) * 4 + c) as usize] = (sum / 4) as u8;
                }
            }
        }
        mips.push((half, next));
    }
    let upload_started = Instant::now();
    let mut tiles = Vec::new();
    let mut ground: Vec<f32> = Vec::new();
    for ty in 0..TILES_Y {
        for tx in 0..TILES_X {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: None,
                size: wgpu::Extent3d { width: TILE_PX, height: TILE_PX, depth_or_array_layers: 1 },
                mip_level_count: mips.len() as u32,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            for (level, (size, data)) in mips.iter().enumerate() {
                queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &texture,
                        mip_level: level as u32,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    data,
                    wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(size * 4), rows_per_image: None },
                    wgpu::Extent3d { width: *size, height: *size, depth_or_array_layers: 1 },
                );
            }
            let view = texture.create_view(&Default::default());
            tiles.push(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &tile_layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&sampler) },
                ],
            }));
            let [x0, y0, x1, y1] = tile_rect(tx, ty);
            for (x, y, u, v) in [(x0, y0, 0.0, 1.0), (x1, y0, 1.0, 1.0), (x1, y1, 1.0, 0.0), (x0, y0, 0.0, 1.0), (x1, y1, 1.0, 0.0), (x0, y1, 0.0, 0.0)] {
                ground.extend_from_slice(&[x, y, 0.0, u, v]);
            }
        }
    }
    queue.submit([]);
    let _ = device.poll(wgpu::PollType::Wait { submission_index: None, timeout: None });
    println!(
        "Tile upload with mips: {:.1} ms per 1024 px tile",
        upload_started.elapsed().as_secs_f64() * 1000.0 / tiles.len() as f64
    );

    let mut verts: Vec<f32> = Vec::new();
    let mut walls = 0;
    for b in &buildings() {
        let n = b.base.len();
        for i in 0..n {
            let (a, c) = (b.base[i], b.base[(i + 1) % n]);
            let s = shade(a, c);
            let col = [s, s, s * 0.97];
            let (h, base) = (b.height, 0.0);
            for p in [[a[0], a[1], base], [c[0], c[1], base], [c[0], c[1], h], [a[0], a[1], base], [c[0], c[1], h], [a[0], a[1], h]] {
                verts.extend_from_slice(&p);
                verts.extend_from_slice(&col);
            }
            walls += 1;
        }
        for i in 1..n - 1 {
            for p in [b.base[0], b.base[i], b.base[i + 1]] {
                verts.extend_from_slice(&[p[0], p[1], b.height, 0.96, 0.95, 0.92]);
            }
        }
    }
    println!("Geometry: {} buildings, {walls} walls, {} triangles", buildings().len(), verts.len() / 18);
    let ground_vb = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(&ground),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let building_vb = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(&verts),
        usage: wgpu::BufferUsages::VERTEX,
    });

    let ground_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[Some(&globals_layout), Some(&tile_layout)],
        immediate_size: 0,
    });
    let wall_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[Some(&globals_layout)],
        immediate_size: 0,
    });
    let ground_attrs = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2];
    let wall_attrs = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3];
    let pipeline = |layout: &wgpu::PipelineLayout, vs: &str, fs: &str, stride: u64, attrs: &[wgpu::VertexAttribute], cull: Option<wgpu::Face>| {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some(vs),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout { array_stride: stride, step_mode: wgpu::VertexStepMode::Vertex, attributes: attrs })],
            },
            primitive: wgpu::PrimitiveState { cull_mode: cull, ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState { count: 4, mask: !0, alpha_to_coverage_enabled: false },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some(fs),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        })
    };
    let ground_pipe = pipeline(&ground_layout, "vs_g", "fs_g", 20, &ground_attrs, None);
    let walls_pipe = pipeline(&wall_layout, "vs_b", "fs_b", 24, &wall_attrs, Some(wgpu::Face::Back));
    let building_verts = (verts.len() / 6) as u32;
    Gpu { device, queue, uniform, globals, ground: ground_pipe, walls: walls_pipe, tiles, ground_vb, building_vb, building_verts }
}

struct Targets {
    msaa: wgpu::TextureView,
    depth: wgpu::TextureView,
    resolve: wgpu::Texture,
    resolve_view: wgpu::TextureView,
}
fn targets(gpu: &Gpu, size: u32) -> Targets {
    let extent = wgpu::Extent3d { width: size, height: size, depth_or_array_layers: 1 };
    let make = |format, samples, usage| {
        gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: extent,
            mip_level_count: 1,
            sample_count: samples,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage,
            view_formats: &[],
        })
    };
    let msaa = make(wgpu::TextureFormat::Rgba8Unorm, 4, wgpu::TextureUsages::RENDER_ATTACHMENT).create_view(&Default::default());
    let depth = make(wgpu::TextureFormat::Depth32Float, 4, wgpu::TextureUsages::RENDER_ATTACHMENT).create_view(&Default::default());
    let resolve = make(wgpu::TextureFormat::Rgba8Unorm, 1, wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC);
    let resolve_view = resolve.create_view(&Default::default());
    Targets { msaa, depth, resolve, resolve_view }
}

/// Records and submits one frame, optionally copying it into `staging`.
fn gpu_frame(gpu: &Gpu, t: &Targets, frame: usize, with_buildings: bool, size: u32, staging: Option<&wgpu::Buffer>) {
    gpu.queue.write_buffer(&gpu.uniform, 0, bytemuck::bytes_of(&clip_matrix(bearing(frame))));
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &t.msaa,
                depth_slice: None,
                resolve_target: Some(&t.resolve_view),
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Discard },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &t.depth,
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Discard }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_bind_group(0, &gpu.globals, &[]);
        pass.set_pipeline(&gpu.ground);
        pass.set_vertex_buffer(0, gpu.ground_vb.slice(..));
        for (i, tile) in gpu.tiles.iter().enumerate() {
            pass.set_bind_group(1, tile, &[]);
            let first = i as u32 * 6;
            pass.draw(first..first + 6, 0..1);
        }
        if with_buildings {
            pass.set_pipeline(&gpu.walls);
            pass.set_vertex_buffer(0, gpu.building_vb.slice(..));
            pass.draw(0..gpu.building_verts, 0..1);
        }
    }
    if let Some(buffer) = staging {
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &t.resolve,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(aligned_row(size)),
                    rows_per_image: None,
                },
            },
            wgpu::Extent3d { width: size, height: size, depth_or_array_layers: 1 },
        );
    }
    gpu.queue.submit([encoder.finish()]);
}
fn aligned_row(size: u32) -> u32 {
    (size * 4).div_ceil(256) * 256
}
fn read_back(gpu: &Gpu, buffer: &wgpu::Buffer, size: u32, out: &mut [u8]) {
    let slice = buffer.slice(..);
    let ready = Arc::new(AtomicBool::new(false));
    let flag = ready.clone();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        result.expect("map");
        flag.store(true, Ordering::SeqCst);
    });
    while !ready.load(Ordering::SeqCst) {
        let _ = gpu.device.poll(wgpu::PollType::Poll);
        std::thread::yield_now();
    }
    {
        let mapped = slice.get_mapped_range().expect("mapped range");
        let (row, stride) = ((size * 4) as usize, aligned_row(size) as usize);
        for y in 0..size as usize {
            out[y * row..(y + 1) * row].copy_from_slice(&mapped[y * stride..y * stride + row]);
        }
    }
    buffer.unmap();
}
fn gpu_case(gpu: &Gpu, size: u32, with_buildings: bool) -> [f64; 3] {
    let t = targets(gpu, size);
    let frames = 300;
    let warm = 30;

    // Render only, waiting for the GPU after each frame.
    let mut started = Instant::now();
    for frame in 0..frames {
        if frame == warm {
            started = Instant::now();
        }
        gpu_frame(gpu, &t, frame, with_buildings, size, None);
        let _ = gpu.device.poll(wgpu::PollType::Wait { submission_index: None, timeout: None });
    }
    let render_ms = started.elapsed().as_secs_f64() * 1000.0 / (frames - warm) as f64;

    let run = |ring: usize| {
        let buffers: Vec<_> = (0..ring)
            .map(|_| {
                gpu.device.create_buffer(&wgpu::BufferDescriptor {
                    label: None,
                    size: (aligned_row(size) * size) as u64,
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                    mapped_at_creation: false,
                })
            })
            .collect();
        let mut out = vec![0_u8; (size * size * 4) as usize];
        let mut pending = vec![false; ring];
        let mut started = Instant::now();
        for frame in 0..frames {
            if frame == warm {
                started = Instant::now();
            }
            let slot = frame % ring;
            if pending[slot] {
                read_back(gpu, &buffers[slot], size, &mut out);
            }
            gpu_frame(gpu, &t, frame, with_buildings, size, Some(&buffers[slot]));
            pending[slot] = true;
            if ring == 1 {
                read_back(gpu, &buffers[0], size, &mut out);
                pending[0] = false;
            }
        }
        started.elapsed().as_secs_f64() * 1000.0 / (frames - warm) as f64
    };
    let blocking_ms = run(1);
    let pipelined_ms = run(4);
    [render_ms, blocking_ms, pipelined_ms]
}

fn skia_images(pixels: &[u8]) -> Vec<sk::Image> {
    let info = sk::ImageInfo::new((TILE_PX as i32, TILE_PX as i32), sk::ColorType::RGBA8888, sk::AlphaType::Premul, None);
    (0..TILES_X * TILES_Y)
        .map(|_| {
            sk::images::raster_from_data(&info, sk::Data::new_copy(pixels), (TILE_PX * 4) as usize).expect("image")
        })
        .collect()
}
fn skia_frame(canvas: &sk::Canvas, images: &[sk::Image], walls: &[Building], size: f32, frame: usize, with_buildings: bool) {
    let clip = clip_matrix(bearing(frame));
    let viewport: M = [
        [size / 2.0, 0.0, 0.0, 0.0],
        [0.0, -size / 2.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [size / 2.0, size / 2.0, 0.0, 1.0],
    ];
    let m = mul(&viewport, &clip);
    let flat: Vec<f32> = m.iter().flatten().copied().collect();
    let flat: [f32; 16] = flat.as_slice().try_into().unwrap();
    let m44 = sk::M44::col_major(&flat);
    canvas.save();
    canvas.concat_44(&m44);
    let sampling = sk::SamplingOptions::new(sk::FilterMode::Linear, sk::MipmapMode::None);
    let paint = sk::Paint::default();
    let mut i = 0;
    for ty in 0..TILES_Y {
        for tx in 0..TILES_X {
            let [x0, y0, x1, y1] = tile_rect(tx, ty);
            canvas.draw_image_rect_with_sampling_options(&images[i], None, sk::Rect::new(x0, y0, x1, y1), sampling, &paint);
            i += 1;
        }
    }
    canvas.restore();
    if !with_buildings {
        return;
    }
    let project = |x: f32, y: f32, z: f32| -> (sk::Point, f32) {
        let w = m[0][3] * x + m[1][3] * y + m[2][3] * z + m[3][3];
        let px = (m[0][0] * x + m[1][0] * y + m[2][0] * z + m[3][0]) / w;
        let py = (m[0][1] * x + m[1][1] * y + m[2][1] * z + m[3][1]) / w;
        (sk::Point::new(px, py), w)
    };
    // Back to front, by projected depth of the footprint's first corner.
    let mut order: Vec<(f32, usize)> = walls
        .iter()
        .enumerate()
        .map(|(i, b)| (project(b.base[0][0], b.base[0][1], 0.0).1, i))
        .collect();
    order.sort_by(|a, b| b.0.total_cmp(&a.0));
    let mut paint = sk::Paint::default();
    paint.set_anti_alias(true);
    let mut roof = Vec::with_capacity(8);
    for (_, index) in order {
        let b = &walls[index];
        let n = b.base.len();
        for i in 0..n {
            let (a, c) = (b.base[i], b.base[(i + 1) % n]);
            let (p0, p1, p2, p3) = (
                project(a[0], a[1], 0.0).0,
                project(c[0], c[1], 0.0).0,
                project(c[0], c[1], b.height).0,
                project(a[0], a[1], b.height).0,
            );
            // Pixel y points down, so a wall facing the camera winds clockwise here.
            let area = (p1.x - p0.x) * (p2.y - p0.y) - (p2.x - p0.x) * (p1.y - p0.y);
            if area >= 0.0 {
                continue;
            }
            let s = shade(a, c);
            paint.set_color4f(sk::Color4f::new(s, s, s * 0.97, 1.0), None);
            canvas.draw_path(&sk::Path::polygon(&[p0, p1, p2, p3], true, None, None), &paint);
        }
        roof.clear();
        roof.extend(b.base.iter().map(|p| project(p[0], p[1], b.height).0));
        paint.set_color4f(sk::Color4f::new(0.96, 0.95, 0.92, 1.0), None);
        canvas.draw_path(&sk::Path::polygon(&roof, true, None, None), &paint);
    }
}
fn skia_case(images: &[sk::Image], size: i32, with_buildings: bool) -> [f64; 2] {
    let walls = buildings();
    let frames = 120;
    let mut surface = sk::surfaces::raster_n32_premul((size, size)).expect("surface");
    let started = Instant::now();
    for frame in 0..frames {
        skia_frame(surface.canvas(), images, &walls, size as f32, frame, with_buildings);
    }
    let single = started.elapsed().as_secs_f64() * 1000.0 / frames as f64;

    let threads = 12;
    let started = Instant::now();
    std::thread::scope(|scope| {
        for worker in 0..threads {
            let walls = &walls;
            let images = images.to_vec();
            scope.spawn(move || {
                let mut surface = sk::surfaces::raster_n32_premul((size, size)).expect("surface");
                for frame in 0..frames {
                    skia_frame(surface.canvas(), &images, walls, size as f32, frame + worker, with_buildings);
                }
            });
        }
    });
    let per_frame = started.elapsed().as_secs_f64() * 1000.0 / (frames * threads) as f64;
    [single, per_frame]
}

fn main() {
    let pixels = tile_pixels();
    let gpu = gpu_setup(&pixels);
    let images = skia_images(&pixels);
    println!("\nms per frame. GPU: 4x MSAA, depth buffer, mipmaps + 8x anisotropy. CPU: Skia, AA, sorted back to front.");
    println!("{:<6}{:<11}{:>12}{:>16}{:>18}{:>12}{:>16}", "size", "scene", "GPU render", "GPU+readback", "GPU pipelined", "Skia 1 thr", "Skia 12 thr");
    for size in [500_u32, 1000] {
        for with_buildings in [false, true] {
            let [render, blocking, pipelined] = gpu_case(&gpu, size, with_buildings);
            let [single, parallel] = skia_case(&images, size as i32, with_buildings);
            println!(
                "{size:<6}{:<11}{render:>12.2}{blocking:>16.2}{pipelined:>18.2}{single:>12.2}{parallel:>16.2}",
                if with_buildings { "buildings" } else { "ground" }
            );
        }
    }
    // Sanity image pair so we can see both scenes really drew the same thing.
    let output = std::env::args().nth(1).expect("output dir");
    let mut surface = sk::surfaces::raster_n32_premul((1000, 1000)).expect("surface");
    skia_frame(surface.canvas(), &images, &buildings(), 1000.0, 0, true);
    let png = surface.image_snapshot().encode(None, sk::EncodedImageFormat::PNG, None).expect("png");
    std::fs::write(format!("{output}/skia.png"), png.as_bytes()).expect("write");
    let t = targets(&gpu, 1000);
    let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: (aligned_row(1000) * 1000) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    gpu_frame(&gpu, &t, 0, true, 1000, Some(&buffer));
    let mut out = vec![0_u8; 4_000_000];
    read_back(&gpu, &buffer, 1000, &mut out);
    let info = sk::ImageInfo::new((1000, 1000), sk::ColorType::RGBA8888, sk::AlphaType::Premul, None);
    let image = sk::images::raster_from_data(&info, sk::Data::new_copy(&out), 4000).expect("image");
    let png = image.encode(None, sk::EncodedImageFormat::PNG, None).expect("png");
    std::fs::write(format!("{output}/gpu.png"), png.as_bytes()).expect("write");
}
