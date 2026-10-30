//! 3D buildings from `fill-extrusion` layers. Nothing exists for this upstream, so it follows
//! maplibre-rs's shape: tessellated once per tile on a worker, drawn per frame from bound slices.
//! Geometry rules follow MapLibre's fill-extrusion bucket. Its lighting is anchored to the viewport,
//! so it is evaluated per frame in `shaders/extrusion.vertex.wgsl` from the normal stored here.

use crate::pattern::EXTENT;
use crate::pipelines::{FillShaderFeatureMetadata, ShaderLayerMetadata};
use crate::style::{evaluate_color, evaluate_number, VectorLayer};
use crate::tile::{
    for_each_passing, DecodedFeature, GeometryKind, LayerKind, PreparedLayer, DEFAULT_TOLERANCE,
};
use bytemuck::{Pod, Zeroable};
use geozero::{error::GeozeroError, GeomProcessor, GeozeroGeometry};
use lyon::geom::point;
use lyon::path::Path;
use lyon::tessellation::{
    BuffersBuilder, FillOptions, FillRule, FillTessellator, FillVertex, FillVertexConstructor,
    VertexBuffers,
};
use maplibre_expr::{EvaluationContext, Expr};

/// The lower bound of MapLibre's wall gradient at its default light intensity of 0.5.
const WALL_GRADIENT_FLOOR: f32 = 0.84;

/// `position` is in tile units on x and y, and metres above the ground on z. `normal` is the wall's
/// unit normal on the ground plane, and zero on roofs. `gradient` darkens walls toward their base.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct ExtrusionVertex {
    pub(crate) position: [f32; 3],
    pub(crate) normal: [f32; 2],
    pub(crate) gradient: f32,
}

struct RoofVertex {
    height: f32,
}

impl FillVertexConstructor<ExtrusionVertex> for RoofVertex {
    fn new_vertex(&mut self, vertex: FillVertex) -> ExtrusionVertex {
        let [x, y] = vertex.position().to_array();
        ExtrusionVertex {
            position: [x, y, self.height],
            normal: [0.0, 0.0],
            gradient: 1.0,
        }
    }
}

/// Tessellates every polygon of a layer into roofs and walls.
pub(crate) fn tessellate_extrusions(
    features: &mut [DecodedFeature<'_>],
    layer: &VectorLayer,
    (color, height, base): (&Expr, Option<&Expr>, Option<&Expr>),
    context: &mut EvaluationContext,
    scale: f32,
    layer_metadata: ShaderLayerMetadata,
) -> Option<PreparedLayer<ExtrusionVertex>> {
    let mut builder = ExtrusionBuilder::new(scale);
    for_each_passing(features, layer.filter.as_ref(), context, |feature, ctx| {
        if feature.kind != GeometryKind::Polygon {
            return;
        }
        let Some(color) = evaluate_color(color, ctx, 1.0) else {
            return;
        };
        builder.begin_feature(
            color,
            evaluate_number(height, ctx, 0.0) as f32,
            evaluate_number(base, ctx, 0.0) as f32,
        );
        if let Err(error) = feature.raw.process_geom(&mut builder) {
            log::debug!("Skipping a building of {}: {error}", layer.id);
            builder.abandon_feature();
        }
    });
    (!builder.buffer.indices.is_empty()).then(|| PreparedLayer {
        index: layer.index,
        kind: LayerKind::Extrusion,
        vertices: builder.buffer.vertices,
        indices: builder.buffer.indices,
        feature_metadata: builder.colors,
        layer_metadata,
    })
}

struct ExtrusionBuilder {
    scale: f32,
    fill: FillTessellator,
    buffer: VertexBuffers<ExtrusionVertex, u32>,
    colors: Vec<FillShaderFeatureMetadata>,
    /// Rings of the polygon being read, closed by a repeated first point.
    rings: Vec<Vec<[f32; 2]>>,
    color: [f32; 4],
    height: f32,
    base: f32,
    feature_start: (usize, usize),
}

impl ExtrusionBuilder {
    fn new(scale: f32) -> Self {
        Self {
            scale,
            fill: FillTessellator::new(),
            buffer: VertexBuffers::new(),
            colors: Vec::new(),
            rings: Vec::new(),
            color: [0.0; 4],
            height: 0.0,
            base: 0.0,
            feature_start: (0, 0),
        }
    }

    fn begin_feature(&mut self, color: [f32; 4], height: f32, base: f32) {
        self.color = color;
        self.height = height;
        self.base = base;
        self.feature_start = (self.buffer.vertices.len(), self.buffer.indices.len());
        self.rings.clear();
    }

    fn abandon_feature(&mut self) {
        self.buffer.vertices.truncate(self.feature_start.0);
        self.buffer.indices.truncate(self.feature_start.1);
        self.colors.truncate(self.feature_start.0);
        self.rings.clear();
    }

    /// The roof by fill tessellation with holes, and one quad per ring edge.
    fn finish_polygon(&mut self) -> Result<(), GeozeroError> {
        let mut path = Path::builder();
        for ring in &self.rings {
            let Some((first, rest)) = ring.split_first() else {
                continue;
            };
            path.begin(point(first[0], first[1]));
            for position in rest {
                path.line_to(point(position[0], position[1]));
            }
            path.end(true);
        }
        self.fill
            .tessellate_path(
                &path.build(),
                &FillOptions::tolerance(DEFAULT_TOLERANCE).with_fill_rule(FillRule::NonZero),
                &mut BuffersBuilder::new(
                    &mut self.buffer,
                    RoofVertex {
                        height: self.height,
                    },
                ),
            )
            .map_err(|error| GeozeroError::Geometry(format!("{error:?}")))?;
        self.colors.resize(
            self.buffer.vertices.len(),
            FillShaderFeatureMetadata { color: self.color },
        );

        for ring_index in 0..self.rings.len() {
            if is_entirely_outside(&self.rings[ring_index]) {
                continue;
            }
            for edge in 1..self.rings[ring_index].len() {
                let (a, b) = (
                    self.rings[ring_index][edge],
                    self.rings[ring_index][edge - 1],
                );
                if !is_boundary_edge(a, b) {
                    self.push_wall(a, b);
                }
            }
        }
        self.rings.clear();
        Ok(())
    }

    fn push_wall(&mut self, a: [f32; 2], b: [f32; 2]) {
        let first = self.buffer.vertices.len() as u32;
        let normal = wall_normal(a, b);
        let [bottom, top] = wall_gradient(self.height, self.base);
        for ([x, y], z, gradient) in [
            (a, self.base, bottom),
            (a, self.height, top),
            (b, self.base, bottom),
            (b, self.height, top),
        ] {
            self.buffer.vertices.push(ExtrusionVertex {
                position: [x, y, z],
                normal,
                gradient,
            });
            self.colors
                .push(FillShaderFeatureMetadata { color: self.color });
        }
        self.buffer.indices.extend_from_slice(&[
            first,
            first + 2,
            first + 1,
            first + 1,
            first + 2,
            first + 3,
        ]);
    }
}

impl GeomProcessor for ExtrusionBuilder {
    fn xy(&mut self, x: f64, y: f64, _idx: usize) -> Result<(), GeozeroError> {
        if let Some(ring) = self.rings.last_mut() {
            ring.push([x as f32 * self.scale, y as f32 * self.scale]);
        }
        Ok(())
    }

    fn linestring_begin(
        &mut self,
        _tagged: bool,
        size: usize,
        _idx: usize,
    ) -> Result<(), GeozeroError> {
        self.rings.push(Vec::with_capacity(size + 1));
        Ok(())
    }

    fn polygon_end(&mut self, _tagged: bool, _idx: usize) -> Result<(), GeozeroError> {
        self.finish_polygon()
    }
}

/// Edges along the tile's clip boundary are not real walls, or tile seams would grow them.
fn is_boundary_edge(a: [f32; 2], b: [f32; 2]) -> bool {
    let (low, high) = (0.0, EXTENT as f32);
    (a[0] == b[0] && (a[0] < low || a[0] > high)) || (a[1] == b[1] && (a[1] < low || a[1] > high))
}

fn is_entirely_outside(ring: &[[f32; 2]]) -> bool {
    let (low, high) = (0.0, EXTENT as f32);
    ring.iter().all(|point| point[0] < low)
        || ring.iter().all(|point| point[0] > high)
        || ring.iter().all(|point| point[1] < low)
        || ring.iter().all(|point| point[1] > high)
}

fn wall_normal(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    let (dx, dy) = (a[0] - b[0], a[1] - b[1]);
    let length = dx.hypot(dy).max(f32::MIN_POSITIVE);
    [-dy / length, dx / length]
}

/// MapLibre's vertical gradient on a wall, at its bottom and top.
fn wall_gradient(height: f32, base: f32) -> [f32; 2] {
    let scale = (height.max(0.0) / 150.0).sqrt();
    [
        (base * scale).clamp(WALL_GRADIENT_FLOOR, 1.0),
        ((1.0 + base) * scale).clamp(WALL_GRADIENT_FLOOR, 1.0),
    ]
}
