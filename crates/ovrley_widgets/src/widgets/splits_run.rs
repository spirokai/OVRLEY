//! Running splits table: split number, pace-proportional bar and pace per split.
//! `ovrley_core` derives the split table; this family only samples and draws it.
use crate::activity::ActivitySampler;
use crate::model::catalog::{DisplayType, MetricKind};
use crate::model::scene::{
    validate_scene_presentation, ScenePresentationConfig, ValidatedScenePresentation,
};
use crate::model::validation::require_non_negative_f32;
use crate::model::validation::{
    require_f32, require_hex_color, require_positive_f32, require_positive_u32, require_string,
    require_unit_opacity, rgba_from_hex,
};
use crate::preview::ResizeGesture;
use crate::preview::WidgetHost;
use crate::render::text::LINE_HEIGHT_RATIO;
use crate::render::text::{
    measure_text_with_font, parse_color, resolve_font, rgba_color, shadow_paint, FontResolver,
    ResolvedTextStyle,
};
use crate::render::tile::{Presented, Tile};
use crate::render::tile::{PreviewRasterScale, StaticRaster};
use crate::render::tile::{TileBounds, TEXT_COLOR_TYPE};
use crate::render::track_fill::{validate_track_fill_color, FillShape, TrackFill, TrackFillColor};
use crate::widgets::lap_timer::layout::TextRun;
use crate::widgets::value::ValueConfig;
use crate::{WidgetError, WidgetResult};
use serde::{Deserialize, Serialize};
use skia_safe::{Canvas, Font, Paint, PathBuilder, PathFillType, Point, RRect, Rect};
use std::sync::{Arc, Mutex};

/// Running bar length resolution in the frame key.
const BAR_TICKS: u32 = 4096;
/// Gap between columns relative to the value font size.
const COLUMN_GAP_RATIO: f32 = 0.6;
/// The split number sits half a column gap further from its bar.
const NUMBER_BAR_GAP_RATIO: f32 = COLUMN_GAP_RATIO * 1.5;
const PLACEHOLDER: &str = "-:--";
/// A running split without a pace draws a full-length bar at this share of its opacity.
const PLACEHOLDER_BAR_OPACITY: f32 = 0.35;

const METERS_PER_KILOMETER: f64 = 1000.0;
const METERS_PER_MILE: f64 = 1609.344;
const METERS_PER_FOOT: f64 = 0.3048;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SplitUnit {
    M,
    Km,
    Ft,
    Mi,
}

impl SplitUnit {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "m" => Some(Self::M),
            "km" => Some(Self::Km),
            "ft" => Some(Self::Ft),
            "mi" => Some(Self::Mi),
            _ => None,
        }
    }

    pub fn meters(self) -> f64 {
        match self {
            Self::M => 1.0,
            Self::Km => METERS_PER_KILOMETER,
            Self::Ft => METERS_PER_FOOT,
            Self::Mi => METERS_PER_MILE,
        }
    }

    /// Metric splits report time per kilometer, imperial splits time per mile.
    pub fn pace_meters(self) -> f64 {
        match self {
            Self::M | Self::Km => METERS_PER_KILOMETER,
            Self::Ft | Self::Mi => METERS_PER_MILE,
        }
    }

    pub fn render_label(self) -> &'static str {
        match self {
            Self::M => "M",
            Self::Km => "KM",
            Self::Ft => "FT",
            Self::Mi => "MI",
        }
    }
}

/// The configured split length; tables are looked up by exact equality.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SplitLength {
    pub distance: f64,
    pub unit: SplitUnit,
}

impl SplitLength {
    pub fn meters(self) -> f64 {
        self.distance * self.unit.meters()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CompletedSplit {
    /// Activity second at which the split's end boundary was crossed.
    pub end_seconds: f64,
    /// Seconds per pace unit ([`SplitUnit::pace_meters`]).
    pub pace_seconds: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SplitTable {
    pub length: SplitLength,
    /// 0-based index of the first split the distance series observes.
    pub first_split: usize,
    /// Completed splits in order.
    pub completed: Vec<CompletedSplit>,
    /// Original-density seconds of the running split's average pace.
    pub live_seconds: Vec<f64>,
    /// Running split's average pace, smoothed within each split; none before it covers distance.
    pub live_pace_seconds: Vec<Option<f64>>,
    /// The pace drawn as a full-length bar.
    pub slowest_pace_seconds: Option<f64>,
}

/// The split running at an activity second.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SplitSample {
    pub completed: usize,
    /// 1-based number of the running split.
    pub number: usize,
    pub pace_seconds: Option<f64>,
}

impl SplitTable {
    /// Splits must be ordered, with finite positive paces, one live pace per activity sample and
    /// a slowest pace to scale the bars when any split has one.
    pub fn validate(&self) -> WidgetResult<()> {
        let invalid = |message: &str| Err(WidgetError::Config(format!("split table: {message}")));
        let positive = |value: f64| value.is_finite() && value > 0.0;
        if !positive(self.length.distance) {
            return invalid("length must be finite and positive");
        }
        if self
            .completed
            .iter()
            .any(|split| !split.end_seconds.is_finite() || !positive(split.pace_seconds))
            || self
                .completed
                .windows(2)
                .any(|pair| pair[0].end_seconds >= pair[1].end_seconds)
        {
            return invalid("completed splits must be ordered with finite positive paces");
        }
        if self.live_seconds.len() != self.live_pace_seconds.len()
            || self.live_seconds.iter().any(|second| !second.is_finite())
            || self.live_seconds.windows(2).any(|pair| pair[0] >= pair[1])
            || self
                .live_pace_seconds
                .iter()
                .flatten()
                .any(|pace| !positive(*pace))
        {
            return invalid("live paces must align with increasing finite seconds");
        }
        let has_pace =
            !self.completed.is_empty() || self.live_pace_seconds.iter().any(Option::is_some);
        match self.slowest_pace_seconds {
            Some(pace) if has_pace && positive(pace) => Ok(()),
            None if !has_pace => Ok(()),
            _ => invalid("slowest pace must be positive exactly when a pace exists"),
        }
    }

    fn completed_before(&self, second: f64) -> usize {
        self.completed
            .partition_point(|split| split.end_seconds <= second + 1e-9)
    }

    pub fn at(&self, second: f64) -> SplitSample {
        let completed = self.completed_before(second);
        SplitSample {
            completed,
            number: self.first_split + completed + 1,
            pace_seconds: self.live_pace_at(completed, second),
        }
    }

    /// Interpolates between the running split's own samples; the edges hold its nearest one.
    fn live_pace_at(&self, completed: usize, second: f64) -> Option<f64> {
        if self
            .live_seconds
            .first()
            .is_none_or(|first| second < *first)
        {
            return None;
        }
        let next = self.live_seconds.partition_point(|time| *time <= second);
        let sample = |index: usize| {
            self.live_seconds.get(index).and_then(|time| {
                (self.completed_before(*time) == completed)
                    .then(|| self.live_pace_seconds[index].map(|pace| (*time, pace)))
                    .flatten()
            })
        };
        match (next.checked_sub(1).and_then(sample), sample(next)) {
            (Some((t0, p0)), Some((t1, p1))) => Some(p0 + (p1 - p0) * (second - t0) / (t1 - t0)),
            (Some((_, pace)), None) | (None, Some((_, pace))) => Some(pace),
            (None, None) => None,
        }
    }

    /// Bar share of the full bar length; paces beyond the slowest clamp to full.
    pub fn bar_fraction(&self, pace_seconds: f64) -> f64 {
        self.slowest_pace_seconds
            .map_or(0.0, |slowest| (pace_seconds / slowest).min(1.0))
    }

    /// Slowest pace any row can print, for stable column widths.
    pub fn slowest_printed_pace(&self) -> Option<f64> {
        self.completed
            .iter()
            .map(|split| split.pace_seconds)
            .chain(self.live_pace_seconds.iter().flatten().copied())
            .reduce(f64::max)
    }

    /// Highest split number any row can print.
    pub fn last_number(&self) -> usize {
        self.first_split + self.completed.len() + 1
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SplitColumns {
    pub split: bool,
    pub bar: bool,
    pub pace: bool,
}

#[derive(Clone, Debug)]
pub struct ValidatedSplitsRun {
    pub x: f32,
    pub y: f32,
    pub font_name: String,
    pub font_size: f32,
    pub color: [u8; 4],
    pub opacity: f32,
    pub label: String,
    pub label_font_name: String,
    pub label_font_size: f32,
    pub label_color: [u8; 4],
    pub length: SplitLength,
    pub max_splits: usize,
    pub columns: SplitColumns,
    pub row_gap: f32,
    /// Full bar length, drawn for the slowest pace; the table width follows its columns.
    pub bar_width: f32,
    pub track_thickness: f32,
    pub track_corner_radius: f32,
    pub track_filled_color: TrackFillColor,
    pub track_filled_opacity: f32,
    /// Zero draws no border, as on linear gauges.
    pub track_border_thickness: f32,
    pub track_border_color: String,
    pub track_border_opacity: f32,
}

pub fn validate_splits_run(value: ValueConfig, index: usize) -> WidgetResult<ValidatedSplitsRun> {
    let p = |name: &str| format!("values[{index}].{name}");
    let invalid =
        |name: &str, message: String| Err(WidgetError::Config(format!("{}: {message}", p(name))));
    if value.value != MetricKind::SplitsRun {
        return invalid("value", "expected splits_run metric".into());
    }
    if value.display_type != DisplayType::SplitsRun {
        return invalid(
            "display_type",
            "splits_run widgets require display_type 'splits_run'".into(),
        );
    }

    let opacity = require_unit_opacity(value.opacity, &p("opacity"))?;
    let color = |field: Option<String>, name: &str| {
        rgba_from_hex(require_string(field, &p(name))?.as_str(), &p(name), opacity)
    };

    let split_distance = value
        .split_distance
        .ok_or_else(|| WidgetError::Config(format!("{}: required", p("split_distance"))))?;
    if !split_distance.is_finite() || split_distance <= 0.0 {
        return invalid(
            "split_distance",
            format!("must be > 0, got {split_distance}"),
        );
    }
    let split_unit = require_string(value.split_unit, &p("split_unit"))?;
    let unit = SplitUnit::parse(&split_unit).ok_or_else(|| {
        WidgetError::Config(format!(
            "{}: unsupported unit '{split_unit}'",
            p("split_unit")
        ))
    })?;

    let mut columns = SplitColumns {
        split: false,
        bar: false,
        pace: false,
    };
    let visible = value
        .visible_columns
        .ok_or_else(|| WidgetError::Config(format!("{}: required", p("visible_columns"))))?;
    if visible.is_empty() {
        return invalid("visible_columns", "must show at least one column".into());
    }
    for column in &visible {
        let shown = match column.as_str() {
            "split" => &mut columns.split,
            "bar" => &mut columns.bar,
            "pace" => &mut columns.pace,
            other => return invalid("visible_columns", format!("unsupported column '{other}'")),
        };
        if *shown {
            return invalid("visible_columns", format!("duplicate column '{column}'"));
        }
        *shown = true;
    }

    let row_gap = require_non_negative_f32(value.row_gap, &p("row_gap"))?;
    let track_thickness = require_positive_f32(value.track_thickness, &p("track_thickness"))?;
    let track_corner_radius =
        require_non_negative_f32(value.track_corner_radius, &p("track_corner_radius"))?;
    let track_border_thickness =
        require_f32(value.track_border_thickness, &p("track_border_thickness"))?;
    if track_border_thickness < 0.0 || track_border_thickness * 2.0 >= track_thickness {
        return invalid(
            "track_border_thickness",
            format!("must be >= 0 and leave a positive fill inside track_thickness, got {track_border_thickness}"),
        );
    }

    Ok(ValidatedSplitsRun {
        x: value.x,
        y: value.y,
        font_name: require_string(value.font, &p("font"))?,
        font_size: require_positive_f32(value.font_size, &p("font_size"))?,
        color: color(value.color, "color")?,
        opacity,
        label: require_string(value.label, &p("label"))?,
        label_font_name: require_string(value.label_font, &p("label_font"))?,
        label_font_size: require_positive_f32(value.label_font_size, &p("label_font_size"))?,
        label_color: color(value.label_color, "label_color")?,
        length: SplitLength {
            distance: split_distance,
            unit,
        },
        max_splits: require_positive_u32(value.max_splits, &p("max_splits"))? as usize,
        columns,
        row_gap,
        bar_width: require_positive_f32(value.bar_width, &p("bar_width"))?,
        track_thickness,
        track_corner_radius,
        track_filled_color: validate_track_fill_color(
            value.track_filled_color.as_deref(),
            value.track_filled_gradient,
            value.track_filled_gradient_stops,
            p,
        )?,
        track_filled_opacity: require_unit_opacity(
            value.track_filled_opacity,
            &p("track_filled_opacity"),
        )?,
        track_border_thickness,
        track_border_color: require_hex_color(
            value.track_border_color.as_deref(),
            &p("track_border_color"),
        )?,
        track_border_opacity: require_unit_opacity(
            value.track_border_opacity,
            &p("track_border_opacity"),
        )?,
    })
}

/// Unitless `M:SS` pace.
pub fn format_pace(pace_seconds: f64) -> String {
    let seconds = pace_seconds.round() as u64;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

/// Header length such as `1 KM`, `0.5 MI` or `400 M`.
pub fn format_split_length(length: SplitLength) -> String {
    let distance = format!("{:.2}", length.distance);
    let distance = distance.trim_end_matches('0').trim_end_matches('.');
    format!("{distance} {}", length.unit.render_label())
}

/// Rounded text and quantized bar length are the presentation cache key.
#[derive(Clone, Debug, PartialEq)]
pub struct SplitsContent {
    pub completed: usize,
    pub number: usize,
    pub pace_text: String,
    /// Running bar length in `1 / BAR_TICKS` of the full bar; absent while the running
    /// split has no pace, which draws the placeholder bar.
    pub bar_ticks: Option<u32>,
}

/// Without a split table (no activity, or its table still in flight) the first split shows a placeholder.
pub fn splits_content(table: Option<&SplitTable>, second: f64) -> SplitsContent {
    let Some(table) = table else {
        return SplitsContent {
            completed: 0,
            number: 1,
            pace_text: PLACEHOLDER.into(),
            bar_ticks: None,
        };
    };
    let sample = table.at(second);
    SplitsContent {
        completed: sample.completed,
        number: sample.number,
        pace_text: sample
            .pace_seconds
            .map_or_else(|| PLACEHOLDER.into(), format_pace),
        bar_ticks: sample
            .pace_seconds
            .map(|pace| (table.bar_fraction(pace) * f64::from(BAR_TICKS)).round() as u32),
    }
}

/// One drawn table row; the running split is always the first.
#[derive(Clone, Debug, PartialEq)]
pub struct SplitRow {
    pub number: usize,
    pub pace_text: String,
    pub bar_fraction: f32,
}

/// Completed rows shown under the running split, newest first, within `max_splits` rows in total.
pub fn completed_rows(
    config: &ValidatedSplitsRun,
    table: Option<&SplitTable>,
    completed: usize,
) -> Vec<SplitRow> {
    let Some(table) = table else {
        return Vec::new();
    };
    table.completed[..completed]
        .iter()
        .enumerate()
        .rev()
        .take(config.max_splits - 1)
        .map(|(index, split)| SplitRow {
            number: table.first_split + index + 1,
            pace_text: format_pace(split.pace_seconds),
            bar_fraction: table.bar_fraction(split.pace_seconds) as f32,
        })
        .collect()
}

/// Bars share one geometry; only their length differs.
#[derive(Clone, Debug)]
struct BarStyle {
    left: f32,
    /// Full length, drawn for the slowest pace.
    length: f32,
    height: f32,
    /// Corner radius within half the bar height; shorter bars round to half their length.
    radius: f32,
    /// Zero draws no border.
    border: f32,
    /// Gradients run along the full bar length, so slower splits reveal more of them.
    fill_paint: Paint,
    border_paint: Paint,
    /// The border casts the shadow when present, otherwise the fill does.
    shadow: Option<(Paint, f32)>,
}

enum BarFill<'a> {
    Paint(&'a Paint),
    /// The running bar reveals a full-length raster rather than shading a gradient every frame.
    Raster(&'a TrackFill),
}

impl BarStyle {
    /// Outer and border-inset shapes of a bar `length` long, built like a linear gauge track.
    fn shapes(&self, top: f32, length: f32) -> (RRect, RRect) {
        let radius = self.radius.min(length * 0.5);
        let outer = RRect::new_rect_xy(
            Rect::from_xywh(self.left, top, length, self.height),
            radius,
            radius,
        );
        let inner_rect = Rect::from_xywh(
            self.left + self.border,
            top + self.border,
            (length - self.border * 2.0).max(0.0),
            (self.height - self.border * 2.0).max(0.0),
        );
        let inner_radius = (radius - self.border).max(0.0);
        (
            outer,
            RRect::new_rect_xy(inner_rect, inner_radius, inner_radius),
        )
    }

    /// The border hugs the bar's own length; it casts the shadow when present, otherwise the fill does.
    fn draw(&self, canvas: &Canvas, top: f32, length: f32, fill: BarFill<'_>) {
        if length <= 0.0 {
            return;
        }
        let (outer, inner) = self.shapes(top, length);
        let inner_rect = *inner.rect();
        // The ring is filled rather than cleared from the outer shape, so bars blend onto what lies beneath.
        let ring = (self.border > 0.0).then(|| {
            let mut path = PathBuilder::new_with_fill_type(PathFillType::EvenOdd);
            path.add_rrect(outer, None, None);
            if !inner_rect.is_empty() {
                path.add_rrect(inner, None, None);
            }
            path.detach()
        });

        if let Some((paint, distance)) = &self.shadow {
            canvas.save();
            canvas.translate((*distance, *distance));
            match &ring {
                Some(ring) => canvas.draw_path(ring, paint),
                None => canvas.draw_rrect(outer, paint),
            };
            canvas.restore();
        }
        if !inner_rect.is_empty() {
            match fill {
                BarFill::Paint(paint) => {
                    canvas.draw_rrect(inner, paint);
                }
                BarFill::Raster(fill) => fill.draw(canvas, &FillShape::RRect(inner), |paint| {
                    canvas.draw_rrect(inner, paint);
                }),
            }
        }
        if let Some(ring) = &ring {
            canvas.draw_path(ring, &self.border_paint);
        }
    }
}

#[derive(Debug)]
struct CachedStatic {
    /// Completed splits, running split number and whether it draws the placeholder bar.
    key: (usize, usize, bool),
    layer: Arc<StaticRaster>,
}

#[derive(Clone, Debug)]
pub struct SplitsLayout {
    config: ValidatedSplitsRun,
    style: ResolvedTextStyle,
    value_font: Font,
    header: Vec<TextRun>,
    number_right: f32,
    pace_right: f32,
    table_right: f32,
    data_top: f32,
    row_height: f32,
    row_stride: f32,
    text_offset: f32,
    bar_offset: f32,
    bar: BarStyle,
    /// Full-length inner bar of the running row.
    running_fill_shape: RRect,
    /// Rasterized on the first frame, so layout-only bounds allocate no surface.
    running_fill: Arc<Mutex<Option<Arc<TrackFill>>>>,
    /// Shadow, text border and antialiasing margin around the content.
    padding: f32,
    // Bounded to the current split state; native workers share the immutable image.
    static_layer: Arc<Mutex<Option<CachedStatic>>>,
}

pub fn validated_splits_style(
    config: &ValidatedSplitsRun,
    scene: &ValidatedScenePresentation,
) -> ResolvedTextStyle {
    ResolvedTextStyle {
        x: config.x,
        y: config.y,
        ..ResolvedTextStyle::widget(
            scene,
            scene.scale,
            Some(config.font_name.clone()),
            config.font_size,
            rgba_color(config.color, 1.0),
            config.opacity,
        )
    }
}

fn digit_count(value: u64) -> usize {
    value.max(1).ilog10() as usize + 1
}

/// Column widths cover every split the table can print, so the table keeps its width.
pub fn prepare_splits_layout(
    config: &ValidatedSplitsRun,
    scene: &ValidatedScenePresentation,
    table: Option<&SplitTable>,
    fonts: &dyn FontResolver,
) -> WidgetResult<SplitsLayout> {
    let scale = scene.scale;
    let style = validated_splits_style(config, scene);
    let label_style = ResolvedTextStyle {
        font_name: Some(config.label_font_name.clone()),
        font_size: config.label_font_size * scale,
        line_height: config.label_font_size * scale * LINE_HEIGHT_RATIO,
        color: rgba_color(config.label_color, 1.0),
        ..style.clone()
    };
    let value_font = resolve_font(fonts, style.font_name.as_deref(), style.font_size)?;
    let label_font = resolve_font(
        fonts,
        label_style.font_name.as_deref(),
        label_style.font_size,
    )?;
    let width = |text: &str, font: &Font| measure_text_with_font(text, font, style.numerals).width;
    let widest_digit = ('0'..='9')
        .map(|digit| width(digit.encode_utf8(&mut [0; 4]), &value_font))
        .fold(0.0, f32::max);
    let number_width =
        widest_digit * digit_count(table.map_or(1, SplitTable::last_number) as u64) as f32;
    let minute_digits = table
        .and_then(SplitTable::slowest_printed_pace)
        .map_or(1, |pace| digit_count(pace.round() as u64 / 60));
    let pace_width = widest_digit * (minute_digits + 2) as f32 + width(":", &value_font);

    let gap = style.font_size * COLUMN_GAP_RATIO;
    let columns = config.columns;
    let label = config.label.to_uppercase();
    let length_text = format_split_length(config.length);
    let label_width = width(&label, &label_font);
    let length_width = width(&length_text, &label_font);
    // Shown columns sit side by side and their widths and gaps make the table width;
    // a wider header widens the table, keeping the pace column at its right edge.
    let number_right = config.x + number_width;
    let bar_left = if columns.split {
        number_right + style.font_size * NUMBER_BAR_GAP_RATIO
    } else {
        config.x
    };
    let track_length = if columns.bar {
        config.bar_width * scale
    } else {
        0.0
    };
    let pace_left = if columns.bar {
        bar_left + track_length + gap
    } else if columns.split {
        number_right + gap
    } else {
        config.x
    };
    let columns_right = if columns.pace {
        pace_left + pace_width
    } else if columns.bar {
        bar_left + track_length
    } else {
        number_right
    };
    let table_right = columns_right.max(config.x + label_width + gap + length_width);
    let mut header = vec![TextRun::cell(
        &length_text,
        table_right,
        config.y,
        &label_style,
        &label_font,
    )?];
    if !label.trim().is_empty() {
        header.push(TextRun::cell(
            &label,
            config.x + label_width,
            config.y,
            &label_style,
            &label_font,
        )?);
    }

    let text_height = style.font_size * LINE_HEIGHT_RATIO;
    let bar_height = config.track_thickness * scale;
    let row_height = [
        (config.columns.split || config.columns.pace).then_some(text_height),
        config.columns.bar.then_some(bar_height),
    ]
    .into_iter()
    .flatten()
    .fold(0.0, f32::max);
    let row_gap = config.row_gap * scale;
    let data_top = config.y + label_style.font_size * LINE_HEIGHT_RATIO + row_gap;

    let opacity = config.opacity;
    let border = config.track_border_thickness * scale;
    // A hidden bar column draws nothing; its gradient still needs a nonzero axis.
    let fill_paint = config.track_filled_color.linear_paint(
        config.track_filled_opacity * opacity,
        Point::new(bar_left + border, 0.0),
        Point::new(
            bar_left + border + (track_length - border * 2.0).max(1.0),
            0.0,
        ),
    )?;
    let mut border_paint = Paint::default();
    border_paint.set_anti_alias(true);
    border_paint.set_color(parse_color(
        &config.track_border_color,
        config.track_border_opacity * opacity,
    ));
    let mut shadow_source = Paint::default();
    shadow_source.set_anti_alias(true);
    let shadow = (!scene.shadow_color.is_empty())
        .then(|| {
            shadow_paint(
                &shadow_source,
                parse_color(&scene.shadow_color, opacity),
                scene.shadow_strength * scale,
            )
        })
        .flatten()
        .map(|paint| (paint, scene.shadow_distance * scale));
    let bar = BarStyle {
        left: bar_left,
        length: track_length,
        height: bar_height,
        radius: (config.track_corner_radius * scale).min(bar_height * 0.5),
        border,
        fill_paint: fill_paint.clone(),
        border_paint,
        shadow,
    };
    let (_, running_fill_shape) =
        bar.shapes(data_top + (row_height - bar_height) * 0.5, bar.length);

    Ok(SplitsLayout {
        config: config.clone(),
        value_font,
        header,
        number_right,
        pace_right: table_right,
        table_right,
        data_top,
        row_height,
        row_stride: row_height + row_gap,
        text_offset: (row_height - text_height) * 0.5,
        bar_offset: (row_height - bar_height) * 0.5,
        padding: (style.shadow_distance.abs() + style.shadow_strength * 3.0)
            .max(scene.shadow_distance.abs() * scale + scene.shadow_strength * scale * 3.0)
            + style.border_thickness
            + 2.0,
        style,
        bar,
        running_fill_shape,
        running_fill: Arc::new(Mutex::new(None)),
        static_layer: Arc::new(Mutex::new(None)),
    })
}

impl SplitsLayout {
    fn row_top(&self, row: usize) -> f32 {
        self.data_top + row as f32 * self.row_stride
    }

    fn cell(&self, text: &str, right: f32, row: usize) -> WidgetResult<TextRun> {
        TextRun::cell(
            text,
            right,
            self.row_top(row) + self.text_offset,
            &self.style,
            &self.value_font,
        )
    }

    /// Header and rows; the running split is row 0.
    fn content_bounds(&self, rows: usize) -> Rect {
        let mut bounds = Rect::new(
            self.config.x,
            self.config.y,
            self.table_right,
            self.row_top(rows) - self.row_stride + self.row_height,
        );
        for run in &self.header {
            bounds.join(run.content);
        }
        bounds
    }

    fn bounds(&self, content: Rect) -> Rect {
        let mut bounds = content;
        bounds.outset((self.padding, self.padding));
        Rect::new(
            bounds.left.floor(),
            bounds.top.floor(),
            bounds.right.ceil(),
            bounds.bottom.ceil(),
        )
    }

    fn draw_row(&self, canvas: &Canvas, row: usize, split: &SplitRow) -> WidgetResult<()> {
        let columns = self.config.columns;
        if columns.split {
            self.cell(&split.number.to_string(), self.number_right, row)?
                .draw(canvas);
        }
        if columns.bar {
            let top = self.row_top(row) + self.bar_offset;
            self.bar.draw(
                canvas,
                top,
                self.bar.length * split.bar_fraction,
                BarFill::Paint(&self.bar.fill_paint),
            );
        }
        if columns.pace {
            self.cell(&split.pace_text, self.pace_right, row)?
                .draw(canvas);
        }
        Ok(())
    }

    /// A running split without a pace caches a faint full-length bar beside its `-:--` pace.
    fn draw_static(
        &self,
        canvas: &Canvas,
        number: usize,
        placeholder: bool,
        rows: &[SplitRow],
    ) -> WidgetResult<()> {
        for run in &self.header {
            run.draw(canvas);
        }
        if self.config.columns.split {
            self.cell(&number.to_string(), self.number_right, 0)?
                .draw(canvas);
        }
        if placeholder && self.config.columns.bar {
            canvas.save_layer_alpha_f(None, PLACEHOLDER_BAR_OPACITY);
            self.bar.draw(
                canvas,
                self.row_top(0) + self.bar_offset,
                self.bar.length,
                BarFill::Paint(&self.bar.fill_paint),
            );
            canvas.restore();
        }
        for (index, row) in rows.iter().enumerate() {
            self.draw_row(canvas, index + 1, row)?;
        }
        Ok(())
    }

    fn running_fill(&self, raster_scale: PreviewRasterScale) -> WidgetResult<Arc<TrackFill>> {
        let mut cache = self.running_fill.lock().expect("splits fill lock poisoned");
        if cache.is_none() {
            let shape = self.running_fill_shape;
            let inner = *shape.rect();
            // A collapsed bar column draws nothing and needs no raster.
            *cache = Some(Arc::new(if inner.is_empty() {
                TrackFill::Paint(self.bar.fill_paint.clone())
            } else {
                TrackFill::prepare(
                    &self.config.track_filled_color,
                    self.bar.fill_paint.clone(),
                    raster_scale,
                    inner,
                    (0.0, 0.0),
                    |canvas, paint| {
                        canvas.draw_rrect(shape, paint);
                    },
                )?
            }));
        }
        Ok(cache.as_ref().expect("running fill prepared").clone())
    }

    fn running_bar_length(&self, ticks: u32) -> f32 {
        self.bar.length * ticks as f32 / BAR_TICKS as f32
    }

    /// Only the running bar and pace repaint while completed rows, header, split number
    /// and the placeholder bar stay cached.
    pub fn prepare_frame(
        &self,
        table: Option<&SplitTable>,
        content: &SplitsContent,
        raster_scale: PreviewRasterScale,
    ) -> WidgetResult<SplitsFrame> {
        let rows = completed_rows(&self.config, table, content.completed);
        let content_bounds = self.content_bounds(rows.len() + 1);
        let bounds = self.bounds(content_bounds);
        let layer = {
            let mut cache = self
                .static_layer
                .lock()
                .expect("splits cache lock poisoned");
            let placeholder = content.bar_ticks.is_none();
            let key = (content.completed, content.number, placeholder);
            if cache.as_ref().map(|cache| cache.key) != Some(key) {
                let layer = StaticRaster::new(
                    raster_scale,
                    Point::default(),
                    bounds,
                    TEXT_COLOR_TYPE,
                    |canvas| self.draw_static(canvas, content.number, placeholder, &rows),
                )?;
                *cache = Some(CachedStatic {
                    key,
                    layer: Arc::new(layer),
                });
            }
            cache
                .as_ref()
                .expect("static split state prepared")
                .layer
                .clone()
        };
        let running_bar = content
            .bar_ticks
            .filter(|ticks| self.config.columns.bar && *ticks > 0)
            .map(|ticks| -> WidgetResult<_> {
                Ok((
                    self.row_top(0) + self.bar_offset,
                    self.running_bar_length(ticks),
                    self.running_fill(raster_scale)?,
                ))
            })
            .transpose()?;
        Ok(SplitsFrame {
            layer,
            running_bar,
            pace: self
                .config
                .columns
                .pace
                .then(|| self.cell(&content.pace_text, self.pace_right, 0))
                .transpose()?,
            content_bounds,
            bounds,
            content_width: self.table_right - self.config.x,
        })
    }

    pub(super) fn draw_running_bar(
        &self,
        canvas: &Canvas,
        top: f32,
        length: f32,
        fill: &TrackFill,
    ) {
        self.bar.draw(canvas, top, length, BarFill::Raster(fill));
    }

    /// Layout-only bounds; no surface is allocated.
    pub fn frame_bounds(
        &self,
        table: Option<&SplitTable>,
        content: &SplitsContent,
    ) -> (Rect, Rect) {
        let rows = completed_rows(&self.config, table, content.completed).len() + 1;
        let content_bounds = self.content_bounds(rows);
        (content_bounds, self.bounds(content_bounds))
    }
}

pub struct SplitsFrame {
    /// Header, completed rows and the running split number.
    pub(super) layer: Arc<StaticRaster>,
    /// Top, length and shared fill of the running bar.
    pub(super) running_bar: Option<(f32, f32, Arc<TrackFill>)>,
    pub(super) pace: Option<TextRun>,
    pub content_bounds: Rect,
    pub bounds: Rect,
    pub content_width: f32,
}

impl SplitsFrame {
    pub fn draw(&self, canvas: &Canvas, layout: &SplitsLayout) {
        self.layer.draw(canvas, None);
        if let Some((top, length, fill)) = &self.running_bar {
            layout.draw_running_bar(canvas, *top, *length, fill);
        }
        if let Some(pace) = &self.pace {
            pace.draw(canvas);
        }
    }
}

#[derive(Serialize)]
pub struct ResizeUpdate {
    pub x: f32,
    pub y: f32,
    pub font_size: f32,
    pub label_font_size: f32,
    pub row_gap: f32,
    pub bar_width: f32,
    pub track_thickness: f32,
    pub track_corner_radius: f32,
    pub track_border_thickness: f32,
}

pub fn resize_splits_run(
    widget: ValueConfig,
    scene: ScenePresentationConfig,
    factor: f32,
    gesture: ResizeGesture,
    activity: Option<&ActivitySampler>,
    fonts: &dyn FontResolver,
) -> WidgetResult<ResizeUpdate> {
    let ResizeGesture {
        fixed,
        point,
        second,
        ..
    } = gesture;
    let mut config = validate_splits_run(widget, 0)?;
    let scene = validate_scene_presentation(scene)?;
    // The font clamp bounds the factor every other dimension follows, so proportions hold.
    let font_size = (config.font_size * factor).clamp(8.0, 400.0);
    let factor = font_size / config.font_size;
    config.font_size = font_size;
    config.label_font_size *= factor;
    config.row_gap *= factor;
    config.bar_width *= factor;
    config.track_thickness *= factor;
    config.track_corner_radius *= factor;
    config.track_border_thickness *= factor;
    let table = activity.and_then(|activity| activity.split_table(config.length));
    let content = splits_content(table, second);
    let (bounds, _) =
        prepare_splits_layout(&config, &scene, table, fonts)?.frame_bounds(table, &content);
    Ok(ResizeUpdate {
        x: config.x + point[0] - (bounds.left + bounds.width() * fixed[0]),
        y: config.y + point[1] - (bounds.top + bounds.height() * fixed[1]),
        font_size: config.font_size,
        label_font_size: config.label_font_size,
        row_gap: config.row_gap,
        bar_width: config.bar_width,
        track_thickness: config.track_thickness,
        track_corner_radius: config.track_corner_radius,
        track_border_thickness: config.track_border_thickness,
    })
}

/// A composited export frame, reused while its content holds.
struct DrawnFrame {
    content: SplitsContent,
    raster: StaticRaster,
    bounds: Rect,
}

pub struct PreviewSplitsRun {
    config: ValidatedSplitsRun,
    scene: ValidatedScenePresentation,
    preview_raster_scale: PreviewRasterScale,
    layout: Option<SplitsLayout>,
    last_content: Option<SplitsContent>,
    /// Export draws onto a restored frame base every frame; equal content blits this raster.
    drawn: Option<DrawnFrame>,
    pub tile: Tile,
}

impl PreviewSplitsRun {
    pub fn new(
        widget: ValueConfig,
        scene: ScenePresentationConfig,
        raster_scale: PreviewRasterScale,
    ) -> WidgetResult<Self> {
        let config = validate_splits_run(widget, 0)?;
        let host = WidgetHost::from_scene(&scene, raster_scale)?;
        Ok(Self::from_validated(
            config,
            validate_scene_presentation(scene)?,
            &host,
        ))
    }

    pub fn from_validated(
        config: ValidatedSplitsRun,
        scene: ValidatedScenePresentation,
        host: &WidgetHost,
    ) -> Self {
        Self {
            config,
            scene,
            preview_raster_scale: host.raster_scale,
            layout: None,
            last_content: None,
            drawn: None,
            tile: Tile::default(),
        }
    }

    /// The table core derived for this widget's split length, once the host has supplied it.
    fn table<'a>(&self, activity: Option<&'a ActivitySampler>) -> Option<&'a SplitTable> {
        activity.and_then(|activity| activity.split_table(self.config.length))
    }

    /// The content the frame at `second` draws; equal content draws identical pixels.
    pub fn frame_key(&self, activity: Option<&ActivitySampler>, second: f64) -> SplitsContent {
        splits_content(self.table(activity), second)
    }

    /// Column widths follow the split table, so a new activity or table rebuilds the layout.
    pub fn invalidate_activity(&mut self) {
        self.layout = None;
        self.last_content = None;
        self.drawn = None;
    }

    fn layout(
        &mut self,
        table: Option<&SplitTable>,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<&SplitsLayout> {
        if self.layout.is_none() {
            self.layout = Some(prepare_splits_layout(
                &self.config,
                &self.scene,
                table,
                fonts,
            )?);
        }
        Ok(self.layout.as_ref().expect("layout prepared"))
    }

    /// Scene rectangle a render at `second` draws into, from layout alone.
    pub fn layout_bounds(
        &mut self,
        activity: Option<&ActivitySampler>,
        second: f64,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<TileBounds> {
        let table = self.table(activity);
        let content = splits_content(table, second);
        Ok(self
            .layout(table, fonts)?
            .frame_bounds(table, &content)
            .1
            .into())
    }

    /// Draws the frame at `second` onto a host canvas in scene coordinates.
    pub fn draw(
        &mut self,
        canvas: &Canvas,
        activity: Option<&ActivitySampler>,
        second: f64,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<TileBounds> {
        let table = self.table(activity);
        let content = splits_content(table, second);
        if self.drawn.as_ref().map(|drawn| &drawn.content) != Some(&content) {
            let raster_scale = self.preview_raster_scale;
            let layout = self.layout(table, fonts)?;
            let frame = layout.prepare_frame(table, &content, raster_scale)?;
            let raster = StaticRaster::new(
                raster_scale,
                Point::default(),
                frame.bounds,
                TEXT_COLOR_TYPE,
                |canvas| {
                    frame.draw(canvas, layout);
                    Ok(())
                },
            )?;
            self.drawn = Some(DrawnFrame {
                content,
                raster,
                bounds: frame.bounds,
            });
        }
        let drawn = self.drawn.as_ref().expect("splits frame drawn");
        drawn.raster.draw(canvas, None);
        Ok(drawn.bounds.into())
    }

    pub fn render(
        &mut self,
        activity: Option<&ActivitySampler>,
        second: f64,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<()> {
        let table = self.table(activity);
        let content = splits_content(table, second);
        if self.last_content.as_ref() == Some(&content) {
            return Ok(());
        }
        self.layout(table, fonts)?;
        let layout = self.layout.as_ref().expect("layout prepared");
        let frame = layout.prepare_frame(table, &content, self.preview_raster_scale)?;
        let bounds = frame.bounds;
        self.tile.render(
            self.preview_raster_scale,
            TEXT_COLOR_TYPE,
            bounds,
            Presented::new(frame.content_bounds, frame.content_width),
            |canvas| {
                frame.draw(canvas, layout);
                Ok(())
            },
        )?;
        self.last_content = Some(content);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{splits_content, CompletedSplit, SplitLength, SplitTable, SplitUnit, BAR_TICKS, *};

    #[test]
    fn pace_and_length_text() {
        assert_eq!(format_pace(299.6), "5:00");
        assert_eq!(format_pace(272.4), "4:32");
        assert_eq!(format_pace(3725.0), "62:05");
        let length = |distance, unit| format_split_length(SplitLength { distance, unit });
        assert_eq!(length(1.0, SplitUnit::Km), "1 KM");
        assert_eq!(length(0.5, SplitUnit::Mi), "0.5 MI");
        assert_eq!(length(400.0, SplitUnit::M), "400 M");
        assert_eq!(length(1.25, SplitUnit::Km), "1.25 KM");
    }

    fn table() -> SplitTable {
        SplitTable {
            length: SplitLength {
                distance: 1.0,
                unit: SplitUnit::Km,
            },
            first_split: 0,
            completed: vec![
                CompletedSplit {
                    end_seconds: 300.0,
                    pace_seconds: 300.0,
                },
                CompletedSplit {
                    end_seconds: 540.0,
                    pace_seconds: 240.0,
                },
            ],
            live_seconds: vec![0.0, 150.0, 299.0, 301.0, 400.0, 541.0, 600.0],
            live_pace_seconds: vec![
                None,
                Some(310.0),
                Some(300.0),
                Some(250.0),
                Some(245.0),
                Some(280.0),
                Some(270.0),
            ],
            slowest_pace_seconds: Some(300.0),
        }
    }

    #[test]
    fn running_split_interpolates_its_own_samples_and_holds_at_edges() {
        let table = table();
        let sample = table.at(75.0);
        assert_eq!((sample.completed, sample.number), (0, 1));
        assert_eq!(sample.pace_seconds, Some(310.0));
        assert_eq!(table.at(299.5).pace_seconds, Some(300.0));
        // Just past a boundary, the next split holds its first sample instead of blending.
        let next = table.at(300.5);
        assert_eq!((next.completed, next.number), (1, 2));
        assert_eq!(next.pace_seconds, Some(250.0));
        assert!((table.at(350.5).pace_seconds.unwrap() - 247.5).abs() < 1e-9);
        // After the last sample the final split holds its last value.
        assert_eq!(table.at(900.0).pace_seconds, Some(270.0));
        assert_eq!(table.at(-1.0).pace_seconds, None);
    }

    #[test]
    fn content_quantizes_bars_against_the_slowest_split() {
        let table = table();
        let content = splits_content(Some(&table), 450.0);
        assert_eq!(content.completed, 1);
        // The next sample belongs to split 3, so split 2 holds its last one.
        assert_eq!(content.pace_text, "4:05");
        assert_eq!(
            content.bar_ticks,
            Some((245.0 / 300.0 * f64::from(BAR_TICKS)).round() as u32)
        );
        let missing = splits_content(None, 450.0);
        assert_eq!(
            (
                missing.number,
                missing.pace_text.as_str(),
                missing.bar_ticks
            ),
            (1, "-:--", None)
        );
        // Before the running split covers distance it shows the same placeholder.
        assert_eq!(splits_content(Some(&table), -1.0).bar_ticks, None);
    }

    #[test]
    fn malformed_tables_fail_at_ingress() {
        assert!(table().validate().is_ok());
        let mut unordered = table();
        unordered.completed.swap(0, 1);
        assert!(unordered.validate().is_err());
        let mut misaligned = table();
        misaligned.live_pace_seconds.pop();
        assert!(misaligned.validate().is_err());
        let mut unscaled = table();
        unscaled.slowest_pace_seconds = None;
        assert!(unscaled.validate().is_err());
    }
}
