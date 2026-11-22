//! Decodes one vector tile and tessellates every style layer it feeds, ported from maplibre-rs
//! `vector/process_vector.rs` and `vector/tessellation.rs`, and lays out its labels.
//!
//! 3D buildings come from `fill-extrusion` layers. Nothing exists for them upstream, so they follow
//! maplibre-rs's shape: tessellated once per tile on a worker, drawn per frame from bound slices.
//! Geometry rules follow MapLibre's fill-extrusion bucket. Its lighting is anchored to the viewport,
//! so it is evaluated per frame in `shaders/extrusion.vertex.wgsl` from the normal stored here.
//!
//! Tile data is external. A malformed feature is logged and skipped, and a layer that fails is
//! dropped, so nothing here panics on it. geozero's reader indexes the geometry without bounds
//! checks, so every feature is walked once here before geozero sees it.

use super::labels::{lay_out, GlyphLookup, LabelSymbol};
use super::pipelines::{FillShaderFeatureMetadata, ShaderDash, ShaderLayerMetadata, ShaderVertex};
use super::style::{
    evaluate_color, evaluate_number, passes_filter, ParsedStyle, VectorLayer, VectorPaint,
};
use bytemuck::{Pod, Zeroable};
use geozero::mvt::{tile, Message, Tile};
use geozero::{error::GeozeroError, GeomProcessor, GeozeroGeometry};
use lyon::geom::point;
use lyon::path::path::Builder;
use lyon::path::Path;
use lyon::tessellation::{
    BuffersBuilder, FillOptions, FillRule, FillTessellator, FillVertex, FillVertexConstructor,
    StrokeOptions, StrokeTessellator, StrokeVertex, StrokeVertexConstructor, VertexBuffers,
};
use maplibre_expr::{EvaluationContext, Expr, Feature, Value};
use std::collections::{BTreeMap, HashMap};

/// Tile units along a tile's edge, whatever extent the tile was cut with: what every tile's
/// geometry and labels are scaled to.
pub(crate) const TILE_UNITS: f64 = 4096.0;
/// Curve flattening tolerance, in tile units.
pub(crate) const DEFAULT_TOLERANCE: f32 = 0.02;
/// Lyon bevels a miter longer than twice this, in half line widths. MapLibre's default
/// `line-miter-limit` of 2 bevels past 2. No preset sets another, so the property isn't read.
const MITER_LIMIT: f32 = 1.0;
/// How far a round join may stray from its circle.
const MAX_ARC_ERROR_PX: f32 = 0.25;
/// The coarsest arc for the thinnest lines: four segments to a half circle.
const MAX_ARC_TOLERANCE: f32 = 0.25;
/// Coordinates beyond this many tile units are not map data.
const MAX_COORDINATE: i64 = 1 << 20;
const COMMAND_MOVE_TO: u32 = 1;
const COMMAND_LINE_TO: u32 = 2;
const COMMAND_CLOSE_PATH: u32 = 7;
/// The lower bound of MapLibre's wall gradient at its default light intensity of 0.5.
const WALL_GRADIENT_FLOOR: f32 = 0.84;

/// One tile's geometry, ready to upload.
#[derive(Default)]
pub(crate) struct PreparedTile {
    pub(crate) ground: Vec<PreparedLayer<ShaderVertex>>,
    pub(crate) extrusions: Vec<PreparedLayer<ExtrusionVertex>>,
    /// The tile's labels, which stay on the CPU. Empty without a glyph source to size them.
    pub(crate) labels: Vec<LabelSymbol>,
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
    /// A fill drawn with this bundled image.
    FillPattern(u8),
    Line,
    Extrusion,
}

/// `position` is in tile units on x and y, and metres above the ground on z. `normal` is the wall's
/// unit normal on the ground plane, and zero on roofs. `gradient` darkens walls toward their base.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct ExtrusionVertex {
    pub(crate) position: [f32; 3],
    pub(crate) normal: [f32; 2],
    pub(crate) gradient: f32,
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

/// The features of each source layer a style reads, decoded once however many layers read them, with
/// the tile's extent.
type SourceFeatures<'t> = HashMap<&'t str, (Vec<DecodedFeature<'t>>, f32)>;

fn source_features<'t, 'c>(
    decoded: &'c mut SourceFeatures<'t>,
    tile: &'t Tile,
    name: &str,
) -> Option<&'c mut (Vec<DecodedFeature<'t>>, f32)> {
    // A style layer whose source layer is absent from this tile simply draws nothing here.
    let source = tile.layers.iter().find(|source| source.name == name)?;
    Some(
        decoded
            .entry(source.name.as_str())
            .or_insert_with(|| (decode_features(source), extent(source))),
    )
}

/// How one ground layer tessellates and draws.
struct GroundStyle<'a> {
    kind: LayerKind,
    color: &'a Expr,
    opacity: Option<&'a Expr>,
    /// A line layer's stroke. It outlines polygons, which a fill layer fills.
    stroke: Option<StrokeOptions>,
    /// Whether line features draw: in a line layer that isn't a fill's outline.
    draws_lines: bool,
    metadata: ShaderLayerMetadata,
}

fn layer_metadata(
    layer: &VectorLayer,
    [line_width, line_blur, line_gap_width]: [f32; 3],
    translate: [f32; 2],
    dash: ShaderDash,
    narrows_with_distance: f32,
) -> ShaderLayerMetadata {
    ShaderLayerMetadata {
        z_index: layer.index as f32,
        line_width,
        line_blur,
        line_gap_width,
        translate,
        narrows_with_distance,
        dash,
    }
}

/// Decodes `bytes`, tessellates every layer of `style` and lays out its labels. Labels need `glyphs`,
/// which a host without font files does not have: its tiles carry none.
pub(crate) fn prepare_tile(
    bytes: &[u8],
    style: &ParsedStyle,
    glyphs: Option<&dyn GlyphLookup>,
) -> Result<PreparedTile, String> {
    let tile = Tile::decode(bytes).map_err(|error| format!("Tile does not decode: {error}"))?;
    let mut decoded = SourceFeatures::new();
    let mut context = EvaluationContext::new().with_zoom(style.zoom);
    let mut ground = ZeroTessellator::default();
    let mut prepared = PreparedTile::default();

    for layer in &style.vector_layers {
        let Some((features, extent)) = source_features(&mut decoded, &tile, &layer.source_layer)
        else {
            continue;
        };
        let scale = TILE_UNITS as f32 / *extent;
        let ground_style = match &layer.paint {
            VectorPaint::Fill {
                color,
                opacity,
                translate,
                pattern,
            } => GroundStyle {
                kind: pattern.map_or(LayerKind::Fill, LayerKind::FillPattern),
                color,
                opacity: opacity.as_ref(),
                stroke: None,
                draws_lines: false,
                metadata: layer_metadata(
                    layer,
                    [1.0, 0.0, 0.0],
                    *translate,
                    ShaderDash::default(),
                    0.0,
                ),
            },
            VectorPaint::Line {
                color,
                opacity,
                width,
                blur,
                gap_width,
                translate,
                polygons_only,
                join,
                dash,
            } => {
                // A line without width draws nothing, and lyon can't stroke it.
                if *width == 0.0 {
                    continue;
                }
                // Lyon decides joins against the width, so it strokes the line as wide as it draws:
                // a cased line's two strokes and the gap between them.
                let drawn_width = if *gap_width > 0.0 {
                    gap_width + 2.0 * width
                } else {
                    *width
                };
                let stroke_width = drawn_width * style.tile_units_per_pixel;
                GroundStyle {
                    kind: LayerKind::Line,
                    color,
                    opacity: opacity.as_ref(),
                    stroke: Some(
                        StrokeOptions::tolerance(arc_tolerance(drawn_width) * stroke_width)
                            .with_line_width(stroke_width)
                            .with_line_join(*join)
                            .with_miter_limit(MITER_LIMIT),
                    ),
                    draws_lines: !polygons_only,
                    // A fill's outline is a hairline, which MapLibre keeps at one pixel everywhere.
                    metadata: layer_metadata(
                        layer,
                        [*width, *blur, *gap_width],
                        *translate,
                        *dash,
                        if *polygons_only { 0.0 } else { 1.0 },
                    ),
                }
            }
            VectorPaint::Extrusion {
                color,
                height,
                base,
            } => {
                let metadata = layer_metadata(
                    layer,
                    [1.0, 0.0, 0.0],
                    [0.0, 0.0],
                    ShaderDash::default(),
                    0.0,
                );
                prepared.extrusions.extend(tessellate_extrusions(
                    features,
                    layer,
                    (color, height.as_ref(), base.as_ref()),
                    &mut context,
                    scale,
                    metadata,
                ));
                continue;
            }
        };
        prepared.ground.extend(ground.tessellate_layer(
            features,
            layer,
            &mut context,
            scale,
            &ground_style,
        ));
    }

    if let Some(glyphs) = glyphs {
        for layer in &style.labels {
            if let Some((features, extent)) =
                source_features(&mut decoded, &tile, &layer.source_layer)
            {
                lay_out(
                    layer,
                    features,
                    *extent,
                    &mut context,
                    glyphs,
                    &mut prepared.labels,
                )?;
            }
        }
    }
    Ok(prepared)
}

/// The stroke tessellator's tolerance for round joins on a line `width` output pixels wide, as a
/// share of that width. A thin line's arcs can be far coarser than a wide road's.
fn arc_tolerance(width: f32) -> f32 {
    (MAX_ARC_ERROR_PX / width.max(f32::MIN_POSITIVE)).min(MAX_ARC_TOLERANCE)
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
        for pair in raw.tags.as_chunks::<2>().0 {
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

fn zigzag(value: u32) -> i64 {
    i64::from((value >> 1) as i32 ^ -((value & 1) as i32))
}

/// The points of a point feature, in the tile's coordinates. [`check_geometry`] has accepted the
/// feature as a single `MoveTo` and its coordinate pairs.
pub(crate) fn point_coordinates(feature: &tile::Feature) -> impl Iterator<Item = [i64; 2]> + '_ {
    let mut cursor = [0_i64; 2];
    feature
        .geometry
        .get(1..)
        .unwrap_or_default()
        .as_chunks::<2>().0.iter()
        .map(move |pair| {
            cursor[0] += zigzag(pair[0]);
            cursor[1] += zigzag(pair[1]);
            cursor
        })
}

/// The parts of a line feature, in the tile's coordinates. [`check_geometry`] has accepted the feature
/// as whole `MoveTo` and `LineTo` commands.
pub(crate) fn line_coordinates(feature: &tile::Feature) -> Vec<Vec<[i64; 2]>> {
    let mut cursor = [0_i64; 2];
    let mut rest = feature.geometry.as_slice();
    let mut parts = Vec::new();
    while !rest.is_empty() {
        let count = (rest[3] >> 3) as usize;
        let mut part = Vec::with_capacity(count + 1);
        cursor[0] += zigzag(rest[1]);
        cursor[1] += zigzag(rest[2]);
        part.push(cursor);
        for pair in rest[4..4 + count * 2].as_chunks::<2>().0 {
            cursor[0] += zigzag(pair[0]);
            cursor[1] += zigzag(pair[1]);
            part.push(cursor);
        }
        parts.push(part);
        rest = &rest[4 + count * 2..];
    }
    parts
}

/// Moves the cursor across one part, returning its surveyor's area. `None` beyond the sane range.
fn advance_cursor(cursor: &mut [i64; 2], first: &[u32], rest: &[u32]) -> Option<i64> {
    cursor[0] += zigzag(first[0]);
    cursor[1] += zigzag(first[1]);
    let start = *cursor;
    let mut area = 0;
    for pair in rest.as_chunks::<2>().0 {
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
            line_distance: 0.0,
        }
    }
}

impl StrokeVertexConstructor<ShaderVertex> for VertexConstructor {
    fn new_vertex(&mut self, vertex: StrokeVertex) -> ShaderVertex {
        ShaderVertex {
            position: vertex.position_on_path().to_array(),
            normal: vertex.normal().to_array(),
            line_distance: vertex.advancement(),
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
    stroke_options: StrokeOptions,
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
            stroke_options: StrokeOptions::default(),
            fill: FillTessellator::new(),
            stroke: StrokeTessellator::new(),
            buffer: VertexBuffers::new(),
        }
    }
}

impl ZeroTessellator {
    /// The features of `layer` that `style` draws, each in its colour. `None` when none draws.
    fn tessellate_layer(
        &mut self,
        features: &mut [DecodedFeature<'_>],
        layer: &VectorLayer,
        context: &mut EvaluationContext,
        scale: f32,
        style: &GroundStyle<'_>,
    ) -> Option<PreparedLayer<ShaderVertex>> {
        self.is_line_layer = style.stroke.is_some();
        self.stroke_options = style.stroke.unwrap_or_default();
        self.scale = scale;
        let mut feature_metadata = Vec::new();
        for_each_passing(
            features,
            layer.filter.as_ref(),
            context,
            |feature, context| {
                let draws = match feature.kind {
                    GeometryKind::Polygon => true,
                    GeometryKind::Line => style.draws_lines,
                    GeometryKind::Point => false,
                };
                if !draws {
                    return;
                }
                let alpha = evaluate_number(style.opacity, context, 1.0);
                let Some(color) = evaluate_color(style.color, context, alpha) else {
                    return;
                };
                match self.tessellate_feature(feature.raw) {
                    Ok(vertices) => feature_metadata.extend(std::iter::repeat_n(
                        FillShaderFeatureMetadata { color },
                        vertices,
                    )),
                    Err(error) => log::debug!("Skipping a feature of {}: {error}", layer.id),
                }
            },
        );
        let buffer = std::mem::replace(&mut self.buffer, VertexBuffers::new());
        (!buffer.indices.is_empty()).then_some(PreparedLayer {
            index: layer.index,
            kind: style.kind,
            vertices: buffer.vertices,
            indices: buffer.indices,
            feature_metadata,
            layer_metadata: style.metadata,
        })
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
                &self.stroke_options,
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

/// Tessellates every polygon of an extrusion layer into roofs and walls.
fn tessellate_extrusions(
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
    (!builder.buffer.indices.is_empty()).then_some(PreparedLayer {
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
    let (low, high) = (0.0, TILE_UNITS as f32);
    (a[0] == b[0] && (a[0] < low || a[0] > high)) || (a[1] == b[1] && (a[1] < low || a[1] > high))
}

fn is_entirely_outside(ring: &[[f32; 2]]) -> bool {
    let (low, high) = (0.0, TILE_UNITS as f32);
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
