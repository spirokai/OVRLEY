//! Shader data layouts and render pipelines, ported from maplibre-rs `render/shaders/mod.rs` and
//! `render/resource/tile_pipeline.rs`, and the bundled `fill-pattern` images they sample.

use super::tile::{ExtrusionVertex, LayerKind};
use bytemuck::{Pod, Zeroable};
use std::mem::size_of;
use wgpu::util::{DeviceExt, TextureDataOrder};

pub(crate) const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
pub(crate) const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth24PlusStencil8;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct ShaderVertex {
    pub(crate) position: [f32; 2],
    pub(crate) normal: [f32; 2],
    /// Tile units along the line, which dashes read. Fills leave it at zero.
    pub(crate) line_distance: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct FillShaderFeatureMetadata {
    pub(crate) color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct ShaderLayerMetadata {
    pub(crate) z_index: f32,
    pub(crate) line_width: f32,
    pub(crate) line_blur: f32,
    pub(crate) line_gap_width: f32,
    /// Style pixels on the map plane.
    pub(crate) translate: [f32; 2],
    /// 1 when a line's width falls with distance, 0 when it holds its pixels.
    pub(crate) narrows_with_distance: f32,
    pub(crate) dash: ShaderDash,
}

/// A line's dashes, in style pixels on the map plane. A solid line has none of these.
#[repr(C)]
#[derive(Clone, Copy, Default, Pod, Zeroable)]
pub(crate) struct ShaderDash {
    pub(crate) on: f32,
    pub(crate) off: f32,
    /// How far a round cap reaches past each end of a dash.
    pub(crate) cap_radius: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct ShaderTileMetadata {
    pub(crate) transform: [[f32; 4]; 4],
    pub(crate) zoom_factor: f32,
    pub(crate) viewport_width: f32,
    pub(crate) viewport_height: f32,
    /// The viewport-anchored light for this frame, on the ground plane in tile axes.
    pub(crate) light_xy: [f32; 2],
    /// This tile's corner in style pixels from the world's, wrapped where every pattern repeats.
    pub(crate) pattern_origin: [f32; 2],
    /// Clip-space w of the map centre, where a line has its nominal width.
    pub(crate) centre_w: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct RouteVertex {
    pub(crate) position: [f32; 2],
    pub(crate) normal: [f32; 2],
    pub(crate) along_metres: f32,
}

/// One route section on one side of the split.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct RouteInstance {
    pub(crate) transform: [[f32; 4]; 4],
    pub(crate) color: [f32; 4],
    pub(crate) viewport_width: f32,
    pub(crate) viewport_height: f32,
    pub(crate) line_width: f32,
    pub(crate) travelled_metres: f32,
    pub(crate) keep_side: f32,
}

/// One corner of a glyph's quad. Positions are final clip-space ones, projected on the CPU.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct LabelVertex {
    pub(crate) position: [f32; 4],
    /// Texels of the glyph atlas.
    pub(crate) texel: [u16; 2],
    pub(crate) fill: [u8; 4],
    pub(crate) halo: [u8; 4],
    /// Output pixels per glyph unit.
    pub(crate) font_scale: f32,
    pub(crate) halo_width: f32,
    pub(crate) halo_blur: f32,
    pub(crate) opacity: f32,
    /// 1 for a Regular standing in for a Bold, 0 otherwise.
    pub(crate) bold: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct BackgroundLayerMetadata {
    pub(crate) color: [f32; 4],
}

/// The haze over the far ground: the style's background colour, laid on by depth.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct FogInstance {
    pub(crate) color: [f32; 4],
    /// The row where the ground is at the map centre's depth, and how the centre's share of a
    /// row's depth changes per row. See `ViewState::ground_depth_rows`.
    pub(crate) depth_rows: [f32; 2],
    /// The centre's share of depth where the haze starts and where it is full, and its strength there.
    pub(crate) haze: [f32; 3],
}

/// One frame's rounded shape, in output pixels.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct ShapeMaskInstance {
    pub(crate) half_size: [f32; 2],
    pub(crate) radius: f32,
}

const SHAPE_MASK_ATTRIBUTES: [wgpu::VertexAttribute; 2] = wgpu::vertex_attr_array![
    0 => Float32x2, // half_size
    1 => Float32, // radius
];

const FOG_ATTRIBUTES: [wgpu::VertexAttribute; 3] = wgpu::vertex_attr_array![
    0 => Float32x4, // color
    1 => Float32x2, // depth_rows
    2 => Float32x3, // haze: start, full, strength
];

const VERTEX_ATTRIBUTES: [wgpu::VertexAttribute; 3] = wgpu::vertex_attr_array![
    0 => Float32x2, // position
    1 => Float32x2, // normal
    2 => Float32, // line_distance
];
const EXTRUSION_VERTEX_ATTRIBUTES: [wgpu::VertexAttribute; 3] = wgpu::vertex_attr_array![
    0 => Float32x3, // position, with metres on z
    1 => Float32x2, // normal
    2 => Float32, // gradient
];
const MASK_TILE_ATTRIBUTES: [wgpu::VertexAttribute; 4] = wgpu::vertex_attr_array![
    4 => Float32x4, // translate
    5 => Float32x4,
    6 => Float32x4,
    7 => Float32x4,
];
const EXTRUSION_TILE_ATTRIBUTES: [wgpu::VertexAttribute; 5] = [
    MASK_TILE_ATTRIBUTES[0],
    MASK_TILE_ATTRIBUTES[1],
    MASK_TILE_ATTRIBUTES[2],
    MASK_TILE_ATTRIBUTES[3],
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x2,
        offset: std::mem::offset_of!(ShaderTileMetadata, light_xy) as u64,
        shader_location: 14,
    },
];
const FILL_TILE_ATTRIBUTES: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![
    4 => Float32x4, // translate
    5 => Float32x4,
    6 => Float32x4,
    7 => Float32x4,
    9 => Float32, // zoom_factor
];
const LINE_TILE_ATTRIBUTES: [wgpu::VertexAttribute; 8] = [
    FILL_TILE_ATTRIBUTES[0],
    FILL_TILE_ATTRIBUTES[1],
    FILL_TILE_ATTRIBUTES[2],
    FILL_TILE_ATTRIBUTES[3],
    FILL_TILE_ATTRIBUTES[4],
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32,
        offset: std::mem::offset_of!(ShaderTileMetadata, viewport_width) as u64,
        shader_location: 11,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32,
        offset: std::mem::offset_of!(ShaderTileMetadata, viewport_height) as u64,
        shader_location: 12,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32,
        offset: std::mem::offset_of!(ShaderTileMetadata, centre_w) as u64,
        shader_location: 3,
    },
];
const PATTERN_TILE_ATTRIBUTES: [wgpu::VertexAttribute; 6] = [
    FILL_TILE_ATTRIBUTES[0],
    FILL_TILE_ATTRIBUTES[1],
    FILL_TILE_ATTRIBUTES[2],
    FILL_TILE_ATTRIBUTES[3],
    FILL_TILE_ATTRIBUTES[4],
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x2,
        offset: std::mem::offset_of!(ShaderTileMetadata, pattern_origin) as u64,
        shader_location: 12,
    },
];
const FILL_LAYER_ATTRIBUTES: [wgpu::VertexAttribute; 2] = [
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32,
        offset: std::mem::offset_of!(ShaderLayerMetadata, z_index) as u64,
        shader_location: 10,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x2,
        offset: std::mem::offset_of!(ShaderLayerMetadata, translate) as u64,
        shader_location: 11,
    },
];
const LINE_LAYER_ATTRIBUTES: [wgpu::VertexAttribute; 3] = wgpu::vertex_attr_array![
    10 => Float32x4, // z_index, line_width, line_blur, line_gap_width
    13 => Float32x3, // translate, narrows_with_distance
    14 => Float32x3, // dash on, off, cap_radius
];
const FEATURE_ATTRIBUTES: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![
    8 => Float32x4, // color
];
const BACKGROUND_ATTRIBUTES: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![
    0 => Float32x4, // color
];

const LABEL_VERTEX_ATTRIBUTES: [wgpu::VertexAttribute; 6] = wgpu::vertex_attr_array![
    0 => Float32x4, // position
    1 => Uint16x2, // texel
    2 => Unorm8x4, // fill
    3 => Unorm8x4, // halo
    4 => Float32x4, // font_scale, halo_width, halo_blur, opacity
    5 => Float32, // bold
];
/// Which pass a label draw makes: 1 for the halo, 0 for the fill.
const LABEL_PASS_ATTRIBUTES: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![
    6 => Float32,
];

const ROUTE_VERTEX_ATTRIBUTES: [wgpu::VertexAttribute; 3] = wgpu::vertex_attr_array![
    0 => Float32x2, // position
    1 => Float32x2, // normal
    2 => Float32, // along_metres
];
const ROUTE_INSTANCE_ATTRIBUTES: [wgpu::VertexAttribute; 10] = wgpu::vertex_attr_array![
    4 => Float32x4, // translate
    5 => Float32x4,
    6 => Float32x4,
    7 => Float32x4,
    8 => Float32x4, // color
    9 => Float32, // viewport_width
    10 => Float32, // viewport_height
    11 => Float32, // line_width
    12 => Float32, // travelled_metres
    13 => Float32, // keep_side
];

const fn instance_layout<'a, T>(
    attributes: &'a [wgpu::VertexAttribute],
) -> wgpu::VertexBufferLayout<'a> {
    wgpu::VertexBufferLayout {
        array_stride: size_of::<T>() as u64,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes,
    }
}

const fn vertex_layout<'a, T>(
    attributes: &'a [wgpu::VertexAttribute],
) -> wgpu::VertexBufferLayout<'a> {
    wgpu::VertexBufferLayout {
        array_stride: size_of::<T>() as u64,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes,
    }
}

/// Blends lines upstream, and fills here so `fill-opacity` and alpha colours work.
const LAYER_BLEND: wgpu::BlendState = wgpu::BlendState {
    color: wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::SrcAlpha,
        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
        operation: wgpu::BlendOperation::Add,
    },
    alpha: wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::One,
        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
        operation: wgpu::BlendOperation::Add,
    },
};

/// Mixes the haze into what is drawn, by its coverage, and leaves the frame's alpha alone, so the
/// transparent outside of a map that doesn't fill its frame stays clear.
const FOG_BLEND: wgpu::BlendState = wgpu::BlendState {
    color: wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::DstAlpha,
        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
        operation: wgpu::BlendOperation::Add,
    },
    alpha: wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::Zero,
        dst_factor: wgpu::BlendFactor::One,
        operation: wgpu::BlendOperation::Add,
    },
};

/// Multiplies what's already drawn by the fragment's alpha, which leaves it premultiplied.
const SHAPE_MASK_BLEND: wgpu::BlendState = wgpu::BlendState {
    color: wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::Zero,
        dst_factor: wgpu::BlendFactor::SrcAlpha,
        operation: wgpu::BlendOperation::Add,
    },
    alpha: wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::Zero,
        dst_factor: wgpu::BlendFactor::SrcAlpha,
        operation: wgpu::BlendOperation::Add,
    },
};

/// The stencil bit water sets, so the haze passes over it. Tile masks write their values below it,
/// and writing them clears it again.
pub(crate) const WATER_STENCIL_BIT: u32 = 0x80;
/// The stencil bits a tile mask's value lives in.
const TILE_STENCIL_MASK: u32 = WATER_STENCIL_BIT - 1;

/// How a tile pipeline uses the stencil.
#[derive(Clone, Copy)]
enum StencilUse {
    /// Writes its reference everywhere it draws: the tile masks.
    Update,
    /// Draws only where the stencil equals its reference: layers.
    Test,
    /// Ignores the stencil: the background.
    Ignore,
    /// Draws each sample once and marks it, so crossings and section seams don't double the opacity.
    Once,
    /// Draws as `Test` does, and sets the water bit where it draws.
    TestAndMarkWater,
    /// Draws only where the water bit is clear: the haze.
    NotWater,
}

impl StencilUse {
    fn state(self) -> wgpu::StencilState {
        let (compare, pass_op, read_mask, write_mask) = match self {
            Self::Update => (
                wgpu::CompareFunction::Always,
                wgpu::StencilOperation::Replace,
                0xff,
                0xff,
            ),
            Self::Test => (
                wgpu::CompareFunction::Equal,
                wgpu::StencilOperation::Keep,
                TILE_STENCIL_MASK,
                0,
            ),
            Self::Ignore => (
                wgpu::CompareFunction::Always,
                wgpu::StencilOperation::Keep,
                0,
                0,
            ),
            Self::Once => (
                wgpu::CompareFunction::NotEqual,
                wgpu::StencilOperation::Replace,
                0xff,
                0xff,
            ),
            Self::TestAndMarkWater => (
                wgpu::CompareFunction::Equal,
                wgpu::StencilOperation::Replace,
                TILE_STENCIL_MASK,
                WATER_STENCIL_BIT,
            ),
            Self::NotWater => (
                wgpu::CompareFunction::Equal,
                wgpu::StencilOperation::Keep,
                WATER_STENCIL_BIT,
                0,
            ),
        };
        let face = wgpu::StencilFaceState {
            compare,
            fail_op: wgpu::StencilOperation::Keep,
            depth_fail_op: wgpu::StencilOperation::Keep,
            pass_op,
        };
        wgpu::StencilState {
            front: face,
            back: face,
            read_mask,
            write_mask,
        }
    }
}

/// Everything one render pass binds.
pub(crate) struct Pipelines {
    pub(crate) mask: wgpu::RenderPipeline,
    pub(crate) background: wgpu::RenderPipeline,
    pub(crate) route: wgpu::RenderPipeline,
    pub(crate) shape_mask: wgpu::RenderPipeline,
    pub(crate) fog: wgpu::RenderPipeline,
    /// Only a style that draws labels has it. Glyphs are not clipped to their tile and ignore depth.
    /// Upright ones draw over the buildings and the route; those lying on the ground before both.
    pub(crate) labels: Option<wgpu::RenderPipeline>,
    /// What a fill pattern's bind group is made for. The label atlas's bind group is made for it too.
    pub(crate) pattern_bind_group_layout: wgpu::BindGroupLayout,
    fill: GroundPipelines,
    fill_pattern: GroundPipelines,
    line: GroundPipelines,
    extrusion: wgpu::RenderPipeline,
}

/// One ground layer kind's pipelines: for water, which marks the stencil so haze passes over it, and
/// for everything else.
struct GroundPipelines {
    land: wgpu::RenderPipeline,
    water: wgpu::RenderPipeline,
}

impl GroundPipelines {
    fn new(pipeline: impl Fn(StencilUse) -> wgpu::RenderPipeline) -> Self {
        Self {
            land: pipeline(StencilUse::Test),
            water: pipeline(StencilUse::TestAndMarkWater),
        }
    }

    const fn get(&self, water: bool) -> &wgpu::RenderPipeline {
        if water {
            &self.water
        } else {
            &self.land
        }
    }
}

struct PipelineBuilder<'a> {
    device: &'a wgpu::Device,
    layout: wgpu::PipelineLayout,
    sample_count: u32,
}

impl PipelineBuilder<'_> {
    fn tile_pipeline(
        &self,
        label: &str,
        vertex: wgpu::ShaderModuleDescriptor<'_>,
        fragment: wgpu::ShaderModuleDescriptor<'_>,
        buffers: &[Option<wgpu::VertexBufferLayout<'_>>],
        target: wgpu::ColorTargetState,
        depth_stencil: wgpu::DepthStencilState,
    ) -> wgpu::RenderPipeline {
        let vertex = self.device.create_shader_module(vertex);
        let fragment = self.device.create_shader_module(fragment);
        self.device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&self.layout),
                vertex: wgpu::VertexState {
                    module: &vertex,
                    entry_point: Some("main"),
                    compilation_options: Default::default(),
                    buffers,
                },
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    front_face: wgpu::FrontFace::Ccw,
                    // Maps look the same from the bottom and above, so no culling is needed.
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: Some(depth_stencil),
                multisample: wgpu::MultisampleState {
                    count: self.sample_count,
                    mask: !0,
                    alpha_to_coverage_enabled: false,
                },
                fragment: Some(wgpu::FragmentState {
                    module: &fragment,
                    entry_point: Some("main"),
                    compilation_options: Default::default(),
                    targets: &[Some(target)],
                }),
                multiview_mask: None,
                cache: None,
            })
    }
}

/// Depth writes off and compare `Always`: draw order is what orders ground layers.
fn ground_depth(stencil: StencilUse) -> wgpu::DepthStencilState {
    wgpu::DepthStencilState {
        format: DEPTH_FORMAT,
        depth_write_enabled: Some(false),
        depth_compare: Some(wgpu::CompareFunction::Always),
        stencil: stencil.state(),
        bias: wgpu::DepthBiasState::default(),
    }
}

impl Pipelines {
    /// `water` picks the variant that keeps haze off what it draws. Extrusions have none.
    pub(crate) const fn layer(&self, kind: LayerKind, water: bool) -> &wgpu::RenderPipeline {
        match kind {
            LayerKind::Fill => self.fill.get(water),
            LayerKind::FillPattern(_) => self.fill_pattern.get(water),
            LayerKind::Line => self.line.get(water),
            LayerKind::Extrusion => &self.extrusion,
        }
    }

    pub(crate) fn new(device: &wgpu::Device, sample_count: u32, draws_labels: bool) -> Self {
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("tile pipeline layout"),
            bind_group_layouts: &[],
            immediate_size: 0,
        });
        let builder = PipelineBuilder {
            device,
            layout,
            sample_count,
        };
        let opaque = wgpu::ColorTargetState {
            format: COLOR_FORMAT,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL,
        };
        let blended = wgpu::ColorTargetState {
            blend: Some(LAYER_BLEND),
            ..opaque.clone()
        };

        let mask = builder.tile_pipeline(
            "mask pipeline",
            wgpu::include_wgsl!("shaders/tile_mask.vertex.wgsl"),
            wgpu::include_wgsl!("shaders/basic.fragment.wgsl"),
            &[Some(instance_layout::<ShaderTileMetadata>(
                &MASK_TILE_ATTRIBUTES,
            ))],
            wgpu::ColorTargetState {
                write_mask: wgpu::ColorWrites::empty(),
                ..opaque.clone()
            },
            ground_depth(StencilUse::Update),
        );
        let fill = GroundPipelines::new(|stencil| {
            builder.tile_pipeline(
                "fill pipeline",
                wgpu::include_wgsl!("shaders/fill.vertex.wgsl"),
                wgpu::include_wgsl!("shaders/fill.fragment.wgsl"),
                &[
                    Some(vertex_layout::<ShaderVertex>(&VERTEX_ATTRIBUTES)),
                    Some(instance_layout::<ShaderTileMetadata>(&FILL_TILE_ATTRIBUTES)),
                    Some(instance_layout::<ShaderLayerMetadata>(
                        &FILL_LAYER_ATTRIBUTES,
                    )),
                    Some(vertex_layout::<FillShaderFeatureMetadata>(
                        &FEATURE_ATTRIBUTES,
                    )),
                ],
                blended.clone(),
                ground_depth(stencil),
            )
        });
        let pattern_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("fill pattern bind group layout"),
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
        let pattern_builder = PipelineBuilder {
            device,
            layout: device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("fill pattern pipeline layout"),
                bind_group_layouts: &[Some(&pattern_bind_group_layout)],
                immediate_size: 0,
            }),
            sample_count,
        };
        let fill_pattern = GroundPipelines::new(|stencil| {
            pattern_builder.tile_pipeline(
                "fill pattern pipeline",
                wgpu::include_wgsl!("shaders/fill_pattern.vertex.wgsl"),
                wgpu::include_wgsl!("shaders/fill_pattern.fragment.wgsl"),
                &[
                    Some(vertex_layout::<ShaderVertex>(&VERTEX_ATTRIBUTES)),
                    Some(instance_layout::<ShaderTileMetadata>(
                        &PATTERN_TILE_ATTRIBUTES,
                    )),
                    Some(instance_layout::<ShaderLayerMetadata>(
                        &FILL_LAYER_ATTRIBUTES,
                    )),
                    Some(vertex_layout::<FillShaderFeatureMetadata>(
                        &FEATURE_ATTRIBUTES,
                    )),
                ],
                blended.clone(),
                ground_depth(stencil),
            )
        });
        let labels = draws_labels.then(|| {
            pattern_builder.tile_pipeline(
                "label pipeline",
                wgpu::include_wgsl!("shaders/label.vertex.wgsl"),
                wgpu::include_wgsl!("shaders/label.fragment.wgsl"),
                &[
                    Some(vertex_layout::<LabelVertex>(&LABEL_VERTEX_ATTRIBUTES)),
                    Some(instance_layout::<f32>(&LABEL_PASS_ATTRIBUTES)),
                ],
                blended.clone(),
                ground_depth(StencilUse::Ignore),
            )
        });
        let line = GroundPipelines::new(|stencil| {
            builder.tile_pipeline(
                "line pipeline",
                wgpu::include_wgsl!("shaders/line.vertex.wgsl"),
                wgpu::include_wgsl!("shaders/line.fragment.wgsl"),
                &[
                    Some(vertex_layout::<ShaderVertex>(&VERTEX_ATTRIBUTES)),
                    Some(instance_layout::<ShaderTileMetadata>(&LINE_TILE_ATTRIBUTES)),
                    Some(instance_layout::<ShaderLayerMetadata>(
                        &LINE_LAYER_ATTRIBUTES,
                    )),
                    Some(vertex_layout::<FillShaderFeatureMetadata>(
                        &FEATURE_ATTRIBUTES,
                    )),
                ],
                blended.clone(),
                ground_depth(stencil),
            )
        });
        let route = builder.tile_pipeline(
            "route pipeline",
            wgpu::include_wgsl!("shaders/route.vertex.wgsl"),
            wgpu::include_wgsl!("shaders/route.fragment.wgsl"),
            &[
                Some(vertex_layout::<RouteVertex>(&ROUTE_VERTEX_ATTRIBUTES)),
                Some(instance_layout::<RouteInstance>(&ROUTE_INSTANCE_ATTRIBUTES)),
            ],
            blended,
            ground_depth(StencilUse::Once),
        );
        let background = builder.tile_pipeline(
            "background pipeline",
            wgpu::include_wgsl!("shaders/background.vertex.wgsl"),
            wgpu::include_wgsl!("shaders/basic.fragment.wgsl"),
            &[Some(instance_layout::<BackgroundLayerMetadata>(
                &BACKGROUND_ATTRIBUTES,
            ))],
            opaque.clone(),
            ground_depth(StencilUse::Ignore),
        );
        let shape_mask = builder.tile_pipeline(
            "shape mask pipeline",
            wgpu::include_wgsl!("shaders/shape_mask.vertex.wgsl"),
            wgpu::include_wgsl!("shaders/shape_mask.fragment.wgsl"),
            &[Some(instance_layout::<ShapeMaskInstance>(
                &SHAPE_MASK_ATTRIBUTES,
            ))],
            wgpu::ColorTargetState {
                blend: Some(SHAPE_MASK_BLEND),
                ..opaque.clone()
            },
            ground_depth(StencilUse::Ignore),
        );
        let fog = builder.tile_pipeline(
            "fog pipeline",
            wgpu::include_wgsl!("shaders/fog.vertex.wgsl"),
            wgpu::include_wgsl!("shaders/fog.fragment.wgsl"),
            &[Some(instance_layout::<FogInstance>(&FOG_ATTRIBUTES))],
            wgpu::ColorTargetState {
                blend: Some(FOG_BLEND),
                ..opaque.clone()
            },
            ground_depth(StencilUse::NotWater),
        );
        let extrusion = builder.tile_pipeline(
            "extrusion pipeline",
            wgpu::include_wgsl!("shaders/extrusion.vertex.wgsl"),
            wgpu::include_wgsl!("shaders/basic.fragment.wgsl"),
            &[
                Some(vertex_layout::<ExtrusionVertex>(
                    &EXTRUSION_VERTEX_ATTRIBUTES,
                )),
                Some(instance_layout::<ShaderTileMetadata>(
                    &EXTRUSION_TILE_ATTRIBUTES,
                )),
                Some(vertex_layout::<FillShaderFeatureMetadata>(
                    &FEATURE_ATTRIBUTES,
                )),
            ],
            opaque,
            // Ground wrote no depth, so buildings only test against each other. The projection is
            // the ordinary 0..1 form, so depth clears to 1.0 and nearer is less.
            wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            },
        );
        Self {
            mask,
            pattern_bind_group_layout,
            fill,
            fill_pattern,
            line,
            background,
            route,
            shape_mask,
            fog,
            labels,
            extrusion,
        }
    }
}

// The `fill-pattern` images, bundled so that drawing them needs no download: raw RGBA8 crops of
// `wetland_bg_11` (30 x 30) and `pedestrian_polygon` (128 x 128) from OpenFreeMap's
// `ofm_f384/ofm@2x.png` sprite (credited in `THIRD_PARTY_NOTICES.txt`).
//
// Each image has its own repeating, mipmapped texture rather than a place in an atlas, because a
// pitched view minifies them far enough to shimmer as the camera moves.

/// Every bundled image is @2x, so a texel is half a style pixel.
struct PatternImage {
    name: &'static str,
    size: u32,
    rgba: &'static [u8],
}

#[cfg(any(target_arch = "wasm32", test))]
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
