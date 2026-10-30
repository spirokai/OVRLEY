//! Decodes one vector tile and tessellates every style layer it feeds, ported from maplibre-rs
//! `vector/process_vector.rs` and `vector/tessellation.rs`.
//!
//! Tile data is external. A malformed feature is logged and skipped, and a layer that fails is
//! dropped, so nothing here panics on it. geozero's reader indexes the geometry without bounds
//! checks, so every feature is walked once here before geozero sees it.

use super::extrusion::{tessellate_extrusions, ExtrusionVertex};
use super::pipelines::{FillShaderFeatureMetadata, ShaderLayerMetadata, ShaderVertex};
use super::style::{evaluate_color, evaluate_number, passes_filter, ParsedStyle, VectorPaint};
use geozero::mvt::{tile, Message, Tile};
use geozero::{error::GeozeroError, GeomProcessor, GeozeroGeometry};
use lyon::geom::point;
use lyon::path::path::Builder;
use lyon::path::Path;
use lyon::tessellation::{
    BuffersBuilder, FillOptions, FillRule, FillTessellator, FillVertex, FillVertexConstructor,
    StrokeOptions, StrokeTessellator, StrokeVertex, StrokeVertexConstructor, VertexBuffers,
};
use maplibre_expr::{EvaluationContext, Feature, Value};
use std::collections::{BTreeMap, HashMap};

/// Curve flattening tolerance, in tile units.
pub(crate) const DEFAULT_TOLERANCE: f32 = 0.02;
/// Coordinates beyond this many tile units are not map data.
const MAX_COORDINATE: i64 = 1 << 20;
const COMMAND_MOVE_TO: u32 = 1;
const COMMAND_LINE_TO: u32 = 2;
const COMMAND_CLOSE_PATH: u32 = 7;

/// One tile's geometry, ready to upload.
#[derive(Default)]
pub(crate) struct PreparedTile {
    pub(crate) ground: Vec<PreparedLayer<ShaderVertex>>,
    pub(crate) extrusions: Vec<PreparedLayer<ExtrusionVertex>>,
}

/// One layer of one tile.
pub(crate) struct PreparedLayer<V> {
    pub(crate) index: u32,
    pub(crate) kind: LayerKind,
    pub(crate) vertices: Vec<V>,
    pub(crate) indices: Vec<u32>,
    pub(crate) feature_metadata: Vec<FillShaderFeatureMetadata>,
    pub(crate) layer_metadata: ShaderLayerMetadata,
}

/// Which pipeline draws a layer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LayerKind {
    Fill,
    Line,
    Extrusion,
}

/// One feature of one source layer: its raw geometry and what expressions read of it.
pub(crate) struct DecodedFeature<'a> {
    pub(crate) raw: &'a tile::Feature,
    pub(crate) properties: Feature,
    pub(crate) kind: GeometryKind,
}

/// The geometry family of a feature.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GeometryKind {
    Point,
    Line,
    Polygon,
}

/// Decodes `bytes` and tessellates every layer of `style`.
pub(crate) fn prepare_tile(bytes: &[u8], style: &ParsedStyle) -> Result<PreparedTile, String> {
    let tile = Tile::decode(bytes).map_err(|error| format!("Tile does not decode: {error}"))?;
    let mut features_by_source: HashMap<&str, (Vec<DecodedFeature<'_>>, f32)> = HashMap::new();
    let mut context = EvaluationContext::new().with_zoom(style.zoom);
    let mut ground_tessellator = ZeroTessellator::default();
    let mut prepared = PreparedTile::default();

    for layer in &style.vector_layers {
        // A style layer whose source layer is absent from this tile simply draws nothing here.
        let Some(source) = tile
            .layers
            .iter()
            .find(|source| source.name == layer.source_layer)
        else {
            continue;
        };
        let (features, scale) = features_by_source
            .entry(layer.source_layer.as_str())
            .or_insert_with(|| (decode_features(source), 4096.0 / extent(source)));

        let metadata = ShaderLayerMetadata {
            z_index: layer.index as f32,
            line_width: match layer.paint {
                VectorPaint::Line { width, .. } => width,
                VectorPaint::Fill { .. } | VectorPaint::Extrusion { .. } => 1.0,
            },
        };
        match &layer.paint {
            VectorPaint::Fill { color, opacity } | VectorPaint::Line { color, opacity, .. } => {
                let (kind, is_line_layer) = match layer.paint {
                    VectorPaint::Line { .. } => (LayerKind::Line, true),
                    VectorPaint::Fill { .. } | VectorPaint::Extrusion { .. } => {
                        (LayerKind::Fill, false)
                    }
                };
                ground_tessellator.begin_layer(is_line_layer, *scale);
                let mut feature_metadata = Vec::new();
                for_each_passing(
                    features,
                    layer.filter.as_ref(),
                    &mut context,
                    |feature, ctx| {
                        let draws = match feature.kind {
                            GeometryKind::Polygon => true,
                            GeometryKind::Line => is_line_layer,
                            GeometryKind::Point => false,
                        };
                        if !draws {
                            return;
                        }
                        let alpha = evaluate_number(opacity.as_ref(), ctx, 1.0);
                        let Some(color) = evaluate_color(color, ctx, alpha) else {
                            return;
                        };
                        match ground_tessellator.tessellate_feature(feature.raw) {
                            Ok(vertices) => feature_metadata.extend(std::iter::repeat_n(
                                FillShaderFeatureMetadata { color },
                                vertices,
                            )),
                            Err(error) => {
                                log::debug!("Skipping a feature of {}: {error}", layer.id);
                            }
                        }
                    },
                );
                let buffer =
                    std::mem::replace(&mut ground_tessellator.buffer, VertexBuffers::new());
                if !buffer.indices.is_empty() {
                    prepared.ground.push(PreparedLayer {
                        index: layer.index,
                        kind,
                        vertices: buffer.vertices,
                        indices: buffer.indices,
                        feature_metadata,
                        layer_metadata: metadata,
                    });
                }
            }
            VectorPaint::Extrusion {
                color,
                height,
                base,
            } => {
                if let Some(extrusion) = tessellate_extrusions(
                    features,
                    layer,
                    (color, height.as_ref(), base.as_ref()),
                    &mut context,
                    *scale,
                    metadata,
                ) {
                    prepared.extrusions.push(extrusion);
                }
            }
        }
    }
    Ok(prepared)
}

/// Calls `visit` with every feature passing `filter`, with `context` set to that feature.
pub(crate) fn for_each_passing<'a>(
    features: &mut [DecodedFeature<'a>],
    filter: Option<&maplibre_expr::Expr>,
    context: &mut EvaluationContext,
    mut visit: impl FnMut(&DecodedFeature<'a>, &EvaluationContext),
) {
    for feature in features {
        std::mem::swap(&mut feature.properties, &mut context.feature);
        if passes_filter(filter, context) {
            visit(feature, context);
        }
        std::mem::swap(&mut feature.properties, &mut context.feature);
    }
}

fn extent(layer: &tile::Layer) -> f32 {
    layer.extent.filter(|extent| *extent > 0).unwrap_or(4096) as f32
}

/// Resolves every feature of a source layer once, however many style layers read it.
fn decode_features(layer: &tile::Layer) -> Vec<DecodedFeature<'_>> {
    let values: Vec<Value> = layer.values.iter().map(column_value).collect();
    let mut features = Vec::with_capacity(layer.features.len());
    for raw in &layer.features {
        let Some(shape) = check_geometry(raw) else {
            log::debug!("Skipping malformed feature in source layer {}", layer.name);
            continue;
        };
        let mut properties = BTreeMap::new();
        for pair in raw.tags.chunks_exact(2) {
            if let (Some(key), Some(value)) = (
                layer.keys.get(pair[0] as usize),
                values.get(pair[1] as usize),
            ) {
                properties.insert(key.clone(), value.clone());
            }
        }
        features.push(DecodedFeature {
            raw,
            properties: Feature {
                id: raw.id.map(|id| Value::Number(id as f64)),
                properties,
                geometry_type: Some(shape.name.to_string()),
                ..Feature::default()
            },
            kind: shape.kind,
        });
    }
    features
}

fn column_value(value: &tile::Value) -> Value {
    if let Some(text) = &value.string_value {
        Value::String(text.clone())
    } else if let Some(number) = value.float_value {
        Value::Number(f64::from(number))
    } else if let Some(number) = value.double_value {
        Value::Number(number)
    } else if let Some(number) = value.int_value {
        Value::Number(number as f64)
    } else if let Some(number) = value.uint_value {
        Value::Number(number as f64)
    } else if let Some(number) = value.sint_value {
        Value::Number(number as f64)
    } else if let Some(flag) = value.bool_value {
        Value::Bool(flag)
    } else {
        Value::Null
    }
}

struct GeometryShape {
    kind: GeometryKind,
    /// The name `["geometry-type"]` reads.
    name: &'static str,
}

/// Walks the geometry commands the way geozero's reader will, returning the shape only when that
/// reader will not index out of bounds.
fn check_geometry(feature: &tile::Feature) -> Option<GeometryShape> {
    let geometry = feature.geometry.as_slice();
    if feature.r#type == Some(tile::GeomType::Point as i32) {
        let (&command, params) = geometry.split_first()?;
        let count = (command >> 3) as usize;
        if command & 7 != COMMAND_MOVE_TO || count == 0 || params.len() != count * 2 {
            return None;
        }
        return Some(GeometryShape {
            kind: GeometryKind::Point,
            name: if count == 1 { "Point" } else { "MultiPoint" },
        });
    }

    let is_polygon = feature.r#type == Some(tile::GeomType::Polygon as i32);
    let is_line = feature.r#type == Some(tile::GeomType::Linestring as i32);
    if !is_polygon && !is_line {
        return None;
    }

    let (mut rest, mut cursor, mut parts) = (geometry, [0_i64; 2], 0_u32);
    while !rest.is_empty() {
        if rest.len() < 4 || rest[0] != (COMMAND_MOVE_TO | 1 << 3) {
            return None;
        }
        let line_to = rest[3];
        let count = (line_to >> 3) as usize;
        if line_to & 7 != COMMAND_LINE_TO || count == 0 {
            return None;
        }
        let params_end = 4 + count * 2;
        let part_end = params_end + usize::from(is_polygon);
        if rest.len() < part_end {
            return None;
        }
        if is_polygon && rest[params_end] != (COMMAND_CLOSE_PATH | 1 << 3) {
            return None;
        }
        let area = advance_cursor(&mut cursor, &rest[1..3], &rest[4..params_end])?;
        // geozero starts a new polygon at each positive ring. A leading hole is an error it returns.
        if is_line || area > 0 {
            parts += 1;
        } else if parts == 0 {
            return None;
        }
        rest = &rest[part_end..];
    }
    if parts == 0 {
        return None;
    }
    Some(if is_line {
        GeometryShape {
            kind: GeometryKind::Line,
            name: if parts == 1 {
                "LineString"
            } else {
                "MultiLineString"
            },
        }
    } else {
        GeometryShape {
            kind: GeometryKind::Polygon,
            name: if parts == 1 {
                "Polygon"
            } else {
                "MultiPolygon"
            },
        }
    })
}

/// Moves the cursor across one part, returning its surveyor's area. `None` beyond the sane range.
fn advance_cursor(cursor: &mut [i64; 2], first: &[u32], rest: &[u32]) -> Option<i64> {
    let zigzag = |value: u32| i64::from((value >> 1) as i32 ^ -((value & 1) as i32));
    cursor[0] += zigzag(first[0]);
    cursor[1] += zigzag(first[1]);
    let start = *cursor;
    let mut area = 0;
    for pair in rest.chunks_exact(2) {
        let previous = *cursor;
        cursor[0] += zigzag(pair[0]);
        cursor[1] += zigzag(pair[1]);
        if cursor[0].abs() > MAX_COORDINATE || cursor[1].abs() > MAX_COORDINATE {
            return None;
        }
        area += previous[0] * cursor[1] - previous[1] * cursor[0];
    }
    Some(area + cursor[0] * start[1] - cursor[1] * start[0])
}

/// Constructor for fill and stroke vertices.
struct VertexConstructor;

impl FillVertexConstructor<ShaderVertex> for VertexConstructor {
    fn new_vertex(&mut self, vertex: FillVertex) -> ShaderVertex {
        ShaderVertex {
            position: vertex.position().to_array(),
            normal: [0.0, 0.0],
        }
    }
}

impl StrokeVertexConstructor<ShaderVertex> for VertexConstructor {
    fn new_vertex(&mut self, vertex: StrokeVertex) -> ShaderVertex {
        ShaderVertex {
            position: vertex.position_on_path().to_array(),
            normal: vertex.normal().to_array(),
        }
    }
}

/// Builds tessellations with vectors, driven by geozero.
struct ZeroTessellator {
    path_builder: Builder,
    path_open: bool,
    is_point: bool,
    /// Polygon geometry draws as outlines when a line layer reads it.
    is_line_layer: bool,
    scale: f32,
    fill: FillTessellator,
    stroke: StrokeTessellator,
    buffer: VertexBuffers<ShaderVertex, u32>,
}

impl Default for ZeroTessellator {
    fn default() -> Self {
        Self {
            path_builder: Path::builder(),
            path_open: false,
            is_point: false,
            is_line_layer: false,
            scale: 1.0,
            fill: FillTessellator::new(),
            stroke: StrokeTessellator::new(),
            buffer: VertexBuffers::new(),
        }
    }
}

impl ZeroTessellator {
    fn begin_layer(&mut self, is_line_layer: bool, scale: f32) {
        self.is_line_layer = is_line_layer;
        self.scale = scale;
    }

    /// Appends one feature, returning how many vertices it added. On failure nothing is kept.
    fn tessellate_feature(&mut self, feature: &tile::Feature) -> Result<usize, GeozeroError> {
        let vertices_before = self.buffer.vertices.len();
        let indices_before = self.buffer.indices.len();
        let result = feature.process_geom(self);
        self.path_builder = Path::builder();
        self.path_open = false;
        if let Err(error) = result {
            self.buffer.vertices.truncate(vertices_before);
            self.buffer.indices.truncate(indices_before);
            return Err(error);
        }
        Ok(self.buffer.vertices.len() - vertices_before)
    }

    fn take_path(&mut self) -> Path {
        std::mem::replace(&mut self.path_builder, Path::builder()).build()
    }

    fn end(&mut self, close: bool) {
        if self.path_open {
            self.path_builder.end(close);
            self.path_open = false;
        }
    }

    fn tessellate_strokes(&mut self) -> Result<(), GeozeroError> {
        let path = self.take_path();
        self.stroke
            .tessellate_path(
                &path,
                &StrokeOptions::tolerance(DEFAULT_TOLERANCE),
                &mut BuffersBuilder::new(&mut self.buffer, VertexConstructor),
            )
            .map(|_| ())
            .map_err(|error| GeozeroError::Geometry(format!("{error:?}")))
    }

    fn tessellate_fill(&mut self) -> Result<(), GeozeroError> {
        let path = self.take_path();
        self.fill
            .tessellate_path(
                &path,
                &FillOptions::tolerance(DEFAULT_TOLERANCE).with_fill_rule(FillRule::NonZero),
                &mut BuffersBuilder::new(&mut self.buffer, VertexConstructor),
            )
            .map(|_| ())
            .map_err(|error| GeozeroError::Geometry(format!("{error:?}")))
    }
}

impl GeomProcessor for ZeroTessellator {
    fn xy(&mut self, x: f64, y: f64, _idx: usize) -> Result<(), GeozeroError> {
        let position = point(x as f32 * self.scale, y as f32 * self.scale);
        if self.is_point {
            // Points draw nothing on the ground.
        } else if self.path_open {
            self.path_builder.line_to(position);
        } else {
            self.path_builder.begin(position);
            self.path_open = true;
        }
        Ok(())
    }

    fn point_begin(&mut self, _idx: usize) -> Result<(), GeozeroError> {
        self.is_point = true;
        Ok(())
    }

    fn point_end(&mut self, _idx: usize) -> Result<(), GeozeroError> {
        self.is_point = false;
        Ok(())
    }

    fn linestring_end(&mut self, tagged: bool, _idx: usize) -> Result<(), GeozeroError> {
        self.end(false);
        if tagged {
            self.tessellate_strokes()?;
        }
        Ok(())
    }

    fn multilinestring_end(&mut self, _idx: usize) -> Result<(), GeozeroError> {
        self.tessellate_strokes()
    }

    fn polygon_end(&mut self, tagged: bool, _idx: usize) -> Result<(), GeozeroError> {
        self.end(true);
        if !tagged {
            return Ok(());
        }
        if self.is_line_layer {
            self.tessellate_strokes()
        } else {
            self.tessellate_fill()
        }
    }

    fn multipolygon_end(&mut self, _idx: usize) -> Result<(), GeozeroError> {
        if self.is_line_layer {
            self.tessellate_strokes()
        } else {
            self.tessellate_fill()
        }
    }
}
