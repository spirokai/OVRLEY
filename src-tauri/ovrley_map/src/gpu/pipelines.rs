//! Shader data layouts and render pipelines, ported from maplibre-rs `render/shaders/mod.rs` and
//! `render/resource/tile_pipeline.rs`.

use super::extrusion::ExtrusionVertex;
use super::tile::LayerKind;
use bytemuck::{Pod, Zeroable};
use std::mem::size_of;

pub(crate) const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
pub(crate) const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth24PlusStencil8;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct ShaderVertex {
    pub(crate) position: [f32; 2],
    pub(crate) normal: [f32; 2],
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

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct BackgroundLayerMetadata {
    pub(crate) color: [f32; 4],
}

const VERTEX_ATTRIBUTES: [wgpu::VertexAttribute; 2] = wgpu::vertex_attr_array![
    0 => Float32x2, // position
    1 => Float32x2, // normal
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
const LINE_TILE_ATTRIBUTES: [wgpu::VertexAttribute; 7] = wgpu::vertex_attr_array![
    4 => Float32x4, // translate
    5 => Float32x4,
    6 => Float32x4,
    7 => Float32x4,
    9 => Float32, // zoom_factor
    11 => Float32, // viewport_width
    12 => Float32, // viewport_height
];
const FILL_LAYER_ATTRIBUTES: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![
    10 => Float32, // z_index
];
const LINE_LAYER_ATTRIBUTES: [wgpu::VertexAttribute; 2] = wgpu::vertex_attr_array![
    10 => Float32, // z_index
    13 => Float32, // line_width
];
const FEATURE_ATTRIBUTES: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![
    8 => Float32x4, // color
];
const BACKGROUND_ATTRIBUTES: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![
    0 => Float32x4, // color
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
}

impl StencilUse {
    fn state(self) -> wgpu::StencilState {
        let (compare, pass_op) = match self {
            Self::Update => (
                wgpu::CompareFunction::Always,
                wgpu::StencilOperation::Replace,
            ),
            Self::Test => (wgpu::CompareFunction::Equal, wgpu::StencilOperation::Keep),
            Self::Ignore => (wgpu::CompareFunction::Always, wgpu::StencilOperation::Keep),
            Self::Once => (
                wgpu::CompareFunction::NotEqual,
                wgpu::StencilOperation::Replace,
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
            read_mask: 0xff,
            write_mask: 0xff,
        }
    }
}

/// Everything one render pass binds.
pub(crate) struct Pipelines {
    pub(crate) mask: wgpu::RenderPipeline,
    pub(crate) background: wgpu::RenderPipeline,
    pub(crate) route: wgpu::RenderPipeline,
    fill: wgpu::RenderPipeline,
    line: wgpu::RenderPipeline,
    extrusion: wgpu::RenderPipeline,
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
    pub(crate) const fn layer(&self, kind: LayerKind) -> &wgpu::RenderPipeline {
        match kind {
            LayerKind::Fill => &self.fill,
            LayerKind::Line => &self.line,
            LayerKind::Extrusion => &self.extrusion,
        }
    }

    pub(crate) fn new(device: &wgpu::Device, sample_count: u32) -> Self {
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
        let fill = builder.tile_pipeline(
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
            ground_depth(StencilUse::Test),
        );
        let line = builder.tile_pipeline(
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
            ground_depth(StencilUse::Test),
        );
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
            fill,
            line,
            background,
            route,
            extrusion,
        }
    }
}
