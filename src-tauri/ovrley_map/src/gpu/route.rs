//! The route as a ground layer. It draws after every other ground layer and before the buildings,
//! so buildings hide it exactly as they hide roads.
//!
//! Unlike a road it comes from the activity, not from tiles, and its colour and width change with
//! the split. So it keeps its own buffers: the line is cut into sections, each with its own origin
//! and matrix, which keeps `f32` precision on long routes and lets far sections be culled per frame.

use super::pipelines::{RouteInstance, RouteVertex};
use super::tile::DEFAULT_TOLERANCE;
use cgmath::{Matrix4, Vector3};
use lyon::geom::point;
use lyon::path::Path;
use lyon::tessellation::{
    BuffersBuilder, LineCap, LineJoin, StrokeOptions, StrokeTessellator, StrokeVertex,
    VertexBuffers,
};
use crate::{MapRouteDraw, MapRouteLine, MapRoutePoint, MapViewport, ViewState};
use std::mem::size_of;
use std::ops::Range;
use std::sync::Arc;
use wgpu::util::DeviceExt;

/// Vertices per section. Sections share their end vertex, so round caps close the seam.
const SECTION_POINTS: usize = 256;
/// Route units along the world's edge: the density of tile units at the deepest tile level.
const ROUTE_UNITS: f64 = 4096.0 * 16384.0;
/// Round joins are built from arcs, whose detail lyon sets from the tolerance against this width.
/// The shader then scales normals to the real width, so this only chooses how round they look.
const TESSELLATION_WIDTH: f32 = 1.0;
/// How far past the frustum's footprint a section still draws, as a share of the footprint.
const BOUNDS_MARGIN: f64 = 0.1;

/// Which side of the split an instance keeps, as the fragment shader tests it.
const KEEP_COMPLETED: f32 = -1.0;
const KEEP_REMAINING: f32 = 1.0;

struct RouteSection {
    indices: Range<u32>,
    origin: (f64, f64),
    min: (f64, f64),
    max: (f64, f64),
}

/// One route on the GPU, with this frame's draws.
pub(crate) struct RouteLayer {
    line: Arc<MapRouteLine>,
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    instances: wgpu::Buffer,
    sections: Vec<RouteSection>,
    frame_instances: Vec<RouteInstance>,
    /// Index range and instance of each draw, completed side first so it stays on top.
    draws: Vec<(Range<u32>, u64)>,
}

impl RouteLayer {
    /// Tessellates and uploads a line. GPS is external data, so a section that cannot tessellate
    /// is logged and skipped.
    pub(crate) fn new(device: &wgpu::Device, line: Arc<MapRouteLine>) -> Self {
        let mut buffers: VertexBuffers<RouteVertex, u32> = VertexBuffers::new();
        let mut tessellator = StrokeTessellator::new();
        let options = StrokeOptions::tolerance(DEFAULT_TOLERANCE)
            .with_line_width(TESSELLATION_WIDTH)
            .with_line_join(LineJoin::Round)
            .with_line_cap(LineCap::Round);
        let mut sections = Vec::new();
        let points = line.points();
        let last = points.len() - 1;
        let mut next_start = 0;
        while next_start < last {
            // Sections share their end vertex, so the line stays connected.
            let start = next_start;
            next_start = (start + SECTION_POINTS - 1).min(last);
            let section_points = &points[start..=next_start];
            let origin = (section_points[0].world_x, section_points[0].world_y);
            let path = section_path(section_points, origin);
            let first_index = buffers.indices.len() as u32;
            let result = tessellator.tessellate_path(
                &path,
                &options,
                &mut BuffersBuilder::new(&mut buffers, |mut vertex: StrokeVertex| {
                    let position = vertex.position_on_path();
                    let normal = vertex.normal();
                    RouteVertex {
                        position: [position.x, position.y],
                        normal: [normal.x, normal.y],
                        along_metres: vertex.interpolated_attributes()[0],
                    }
                }),
            );
            if let Err(error) = result {
                log::warn!("Skipping a map route section: {error:?}");
                buffers.indices.truncate(first_index as usize);
                continue;
            }
            let (mut min, mut max) = (origin, origin);
            for point in section_points {
                min = (min.0.min(point.world_x), min.1.min(point.world_y));
                max = (max.0.max(point.world_x), max.1.max(point.world_y));
            }
            sections.push(RouteSection {
                indices: first_index..buffers.indices.len() as u32,
                origin,
                min,
                max,
            });
        }

        let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("route vertex buffer"),
            contents: bytemuck::cast_slice(&buffers.vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let indices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("route index buffer"),
            contents: bytemuck::cast_slice(&buffers.indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        let instances = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("route instance buffer"),
            size: (sections.len().max(1) * 2 * size_of::<RouteInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            line,
            vertices,
            indices,
            instances,
            frame_instances: Vec::with_capacity(sections.len() * 2),
            draws: Vec::with_capacity(sections.len() * 2),
            sections,
        }
    }

    pub(crate) fn draws_line(&self, line: &Arc<MapRouteLine>) -> bool {
        Arc::ptr_eq(&self.line, line)
    }

    pub(crate) fn clear(&mut self) {
        self.draws.clear();
    }

    /// Chooses this frame's sections and uploads their matrices, computed in `f64` per section.
    pub(crate) fn prepare(
        &mut self,
        queue: &wgpu::Queue,
        view: &ViewState,
        viewport: MapViewport,
        route: &MapRouteDraw,
    ) -> Result<(), String> {
        self.clear();
        self.frame_instances.clear();
        let Some((min, max)) = view.ground_bounds() else {
            return Ok(());
        };
        let margin = (
            (max.0 - min.0) * BOUNDS_MARGIN,
            (max.1 - min.1) * BOUNDS_MARGIN,
        );
        let view_projection = view.view_projection();
        let world_size = view.world_size();
        let visible: Vec<usize> = self
            .sections
            .iter()
            .enumerate()
            .filter(|(_, section)| {
                section.max.0 >= min.0 - margin.0
                    && section.min.0 <= max.0 + margin.0
                    && section.max.1 >= min.1 - margin.1
                    && section.min.1 <= max.1 + margin.1
            })
            .map(|(index, _)| index)
            .collect();
        let style = route.style;
        for (color, width, side) in [
            (
                style.completed_color,
                style.completed_width_px,
                KEEP_COMPLETED,
            ),
            (
                style.remaining_color,
                style.remaining_width_px,
                KEEP_REMAINING,
            ),
        ] {
            for &index in &visible {
                let section = &self.sections[index];
                let transform = view_projection
                    * Matrix4::from_translation(Vector3::new(
                        section.origin.0 * world_size,
                        section.origin.1 * world_size,
                        0.0,
                    ))
                    * Matrix4::from_scale(world_size / ROUTE_UNITS);
                let transform = transform
                    .cast::<f32>()
                    .ok_or("Route matrix does not fit in f32")?;
                self.draws
                    .push((section.indices.clone(), self.frame_instances.len() as u64));
                self.frame_instances.push(RouteInstance {
                    transform: transform.into(),
                    color,
                    viewport_width: viewport.width_px() as f32,
                    viewport_height: viewport.height_px() as f32,
                    line_width: width,
                    travelled_metres: route.travelled_metres,
                    keep_side: side,
                });
            }
        }
        if !self.frame_instances.is_empty() {
            queue.write_buffer(
                &self.instances,
                0,
                bytemuck::cast_slice(&self.frame_instances),
            );
        }
        Ok(())
    }

    /// Records the frame's draws. The pipeline must already be bound.
    pub(crate) fn record(&self, pass: &mut wgpu::RenderPass<'_>) {
        if self.draws.is_empty() {
            return;
        }
        let stride = size_of::<RouteInstance>() as u64;
        pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint32);
        pass.set_vertex_buffer(0, self.vertices.slice(..));
        for (indices, instance) in &self.draws {
            pass.set_vertex_buffer(
                1,
                self.instances
                    .slice(instance * stride..(instance + 1) * stride),
            );
            pass.draw_indexed(indices.clone(), 0, 0..1);
        }
    }

    pub(crate) fn has_draws(&self) -> bool {
        !self.draws.is_empty()
    }
}

/// One section's stroke path relative to its origin, carrying travelled distance as an attribute.
fn section_path(points: &[MapRoutePoint], origin: (f64, f64)) -> Path {
    let position = |point_at: &MapRoutePoint| {
        point(
            ((point_at.world_x - origin.0) * ROUTE_UNITS) as f32,
            ((point_at.world_y - origin.1) * ROUTE_UNITS) as f32,
        )
    };
    let mut builder = Path::builder_with_attributes(1);
    builder.begin(position(&points[0]), &[points[0].along_metres]);
    for point_at in &points[1..] {
        builder.line_to(position(point_at), &[point_at.along_metres]);
    }
    builder.end(false);
    builder.build()
}
