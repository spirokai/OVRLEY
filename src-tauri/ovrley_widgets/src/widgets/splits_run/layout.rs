//! Stable split columns, linear-style bars and the cached layer of everything but the running split.
use super::{
    format::{completed_rows, format_split_length, SplitRow, SplitsContent},
    frame::SplitsFrame,
    table::SplitTable,
    validation::ValidatedSplitsRun,
    BAR_TICKS, COLUMN_GAP_RATIO, NUMBER_BAR_GAP_RATIO, PLACEHOLDER_BAR_OPACITY,
};
use crate::{
    error::WidgetResult,
    model::{scene::ValidatedScenePresentation, standard_widgets::widget_font_weight},
    render::{
        preview::{PreviewRasterScale, StaticRaster},
        shadow::shadow_paint,
        text::{measure_text_with_font, parse_color, resolve_font, rgba_color, FontResolver, ResolvedTextStyle},
        track_fill::{FillShape, TrackFill},
    },
    widgets::{
        lap_timer::{frame::TextRun, LINE_HEIGHT_RATIO},
        value::preview::TEXT_COLOR_TYPE,
    },
};
use skia_safe::{Canvas, Font, Paint, PathBuilder, PathFillType, Point, RRect, Rect};
use std::sync::{Arc, Mutex};

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
        let outer = RRect::new_rect_xy(Rect::from_xywh(self.left, top, length, self.height), radius, radius);
        let inner_rect = Rect::from_xywh(
            self.left + self.border,
            top + self.border,
            (length - self.border * 2.0).max(0.0),
            (self.height - self.border * 2.0).max(0.0),
        );
        let inner_radius = (radius - self.border).max(0.0);
        (outer, RRect::new_rect_xy(inner_rect, inner_radius, inner_radius))
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

pub fn validated_splits_style(config: &ValidatedSplitsRun, scene: &ValidatedScenePresentation) -> ResolvedTextStyle {
    let opacity = config.opacity;
    ResolvedTextStyle {
        x: config.x,
        y: config.y,
        font_name: Some(config.font_name.clone()),
        font_size: config.font_size * scene.scale,
        font_weight: widget_font_weight(),
        italic: false,
        letter_spacing: 0.0,
        line_height: config.font_size * scene.scale * LINE_HEIGHT_RATIO,
        color: rgba_color(config.color, 1.0),
        opacity,
        shadow_color: (!scene.shadow_color.is_empty()).then(|| parse_color(&scene.shadow_color, opacity)),
        shadow_strength: scene.shadow_strength * scene.scale,
        shadow_distance: scene.shadow_distance * scene.scale,
        border_color: (!scene.border_color.is_empty()).then(|| parse_color(&scene.border_color, opacity)),
        border_thickness: scene.border_thickness * scene.scale,
        numerals: scene.numerals,
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
    let label_font = resolve_font(fonts, label_style.font_name.as_deref(), label_style.font_size)?;
    let width = |text: &str, font: &Font| measure_text_with_font(text, font, style.numerals).width;
    let widest_digit = ('0'..='9')
        .map(|digit| width(digit.encode_utf8(&mut [0; 4]), &value_font))
        .fold(0.0, f32::max);
    let number_width = widest_digit * digit_count(table.map_or(1, SplitTable::last_number) as u64) as f32;
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
    let bar_left = if columns.split { number_right + style.font_size * NUMBER_BAR_GAP_RATIO } else { config.x };
    let track_length = if columns.bar { config.bar_width * scale } else { 0.0 };
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
    let mut header = vec![TextRun::cell(&length_text, table_right, config.y, &label_style, &label_font)?];
    if !label.trim().is_empty() {
        header.push(TextRun::cell(&label, config.x + label_width, config.y, &label_style, &label_font)?);
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
        Point::new(bar_left + border + (track_length - border * 2.0).max(1.0), 0.0),
    )?;
    let mut border_paint = Paint::default();
    border_paint.set_anti_alias(true);
    border_paint.set_color(parse_color(&config.track_border_color, config.track_border_opacity * opacity));
    let mut shadow_source = Paint::default();
    shadow_source.set_anti_alias(true);
    let shadow = (!scene.shadow_color.is_empty())
        .then(|| shadow_paint(&shadow_source, parse_color(&scene.shadow_color, opacity), scene.shadow_strength * scale))
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
    let (_, running_fill_shape) = bar.shapes(data_top + (row_height - bar_height) * 0.5, bar.length);

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
        TextRun::cell(text, right, self.row_top(row) + self.text_offset, &self.style, &self.value_font)
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
        Rect::new(bounds.left.floor(), bounds.top.floor(), bounds.right.ceil(), bounds.bottom.ceil())
    }

    fn draw_row(&self, canvas: &Canvas, row: usize, split: &SplitRow) -> WidgetResult<()> {
        let columns = self.config.columns;
        if columns.split {
            self.cell(&split.number.to_string(), self.number_right, row)?.draw(canvas);
        }
        if columns.bar {
            let top = self.row_top(row) + self.bar_offset;
            self.bar.draw(canvas, top, self.bar.length * split.bar_fraction, BarFill::Paint(&self.bar.fill_paint));
        }
        if columns.pace {
            self.cell(&split.pace_text, self.pace_right, row)?.draw(canvas);
        }
        Ok(())
    }

    /// A running split without a pace caches a faint full-length bar beside its `-:--` pace.
    fn draw_static(&self, canvas: &Canvas, number: usize, placeholder: bool, rows: &[SplitRow]) -> WidgetResult<()> {
        for run in &self.header {
            run.draw(canvas);
        }
        if self.config.columns.split {
            self.cell(&number.to_string(), self.number_right, 0)?.draw(canvas);
        }
        if placeholder && self.config.columns.bar {
            canvas.save_layer_alpha_f(None, PLACEHOLDER_BAR_OPACITY);
            self.bar.draw(canvas, self.row_top(0) + self.bar_offset, self.bar.length, BarFill::Paint(&self.bar.fill_paint));
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
            let mut cache = self.static_layer.lock().expect("splits cache lock poisoned");
            let placeholder = content.bar_ticks.is_none();
            let key = (content.completed, content.number, placeholder);
            if cache.as_ref().map(|cache| cache.key) != Some(key) {
                let layer = StaticRaster::new(raster_scale, Point::default(), bounds, TEXT_COLOR_TYPE, |canvas| {
                    self.draw_static(canvas, content.number, placeholder, &rows)
                })?;
                *cache = Some(CachedStatic { key, layer: Arc::new(layer) });
            }
            cache.as_ref().expect("static split state prepared").layer.clone()
        };
        let running_bar = content
            .bar_ticks
            .filter(|ticks| self.config.columns.bar && *ticks > 0)
            .map(|ticks| -> WidgetResult<_> {
                Ok((self.row_top(0) + self.bar_offset, self.running_bar_length(ticks), self.running_fill(raster_scale)?))
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

    pub(super) fn draw_running_bar(&self, canvas: &Canvas, top: f32, length: f32, fill: &TrackFill) {
        self.bar.draw(canvas, top, length, BarFill::Raster(fill));
    }

    /// Layout-only bounds; no surface is allocated.
    pub fn frame_bounds(&self, table: Option<&SplitTable>, content: &SplitsContent) -> (Rect, Rect) {
        let rows = completed_rows(&self.config, table, content.completed).len() + 1;
        let content_bounds = self.content_bounds(rows);
        (content_bounds, self.bounds(content_bounds))
    }
}
