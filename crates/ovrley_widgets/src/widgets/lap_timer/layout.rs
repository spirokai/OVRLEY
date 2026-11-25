//! Lap timer layout and drawing.
use crate::model::scene::ValidatedScenePresentation;
use crate::render::text::LINE_HEIGHT_RATIO;
use crate::render::text::{
    baseline_for_text_top_with_line_height, draw_text_blob, layout_text, measure_text_with_font,
    resolve_font, rgba_color, text_selection_bounds, FontResolver, ResolvedTextStyle,
};
use crate::render::tile::TEXT_COLOR_TYPE;
use crate::render::tile::{PreviewRasterScale, StaticRaster};
use crate::widgets::lap_timer::{
    best_completed_lap, delta_color, format_lap_delta, format_lap_duration, LapHistory,
    LapTimerContent, LapTimerMode, ValidatedLapTimer, LOG_COLUMN_GAP_RATIO, LOG_HEADER_OPACITY,
    LOG_ROW_GAP_RATIO, PLACEHOLDER, TABLE_VERTICAL_METRICS_TEXT,
};
use crate::{WidgetError, WidgetResult};
use skia_safe::{Canvas, Font, Point, Rect, TextBlob};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, PartialEq)]
enum StaticContent {
    Label,
    Log {
        completed: usize,
        current_number: Option<String>,
    },
}

#[derive(Debug)]
struct CachedStatic {
    content: StaticContent,
    layer: Option<Arc<TextLayer>>,
}

#[derive(Clone, Debug)]
pub struct LapTimerLayout {
    config: ValidatedLapTimer,
    style: ResolvedTextStyle,
    value_font: Font,
    label: Option<TextRun>,
    best_values: BTreeMap<String, TextRun>,
    headers: Vec<TextRun>,
    completed_rows: Vec<[TextRun; 2]>,
    column_rights: [f32; 3],
    data_top: f32,
    row_stride: f32,
    // Bounded to the current lap state; native workers share the immutable image.
    static_layer: Arc<Mutex<Option<CachedStatic>>>,
}

pub fn validated_lap_timer_style(
    config: &ValidatedLapTimer,
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

pub fn prepare_lap_timer_layout(
    config: &ValidatedLapTimer,
    scene: &ValidatedScenePresentation,
    history: LapHistory<'_>,
    fonts: &dyn FontResolver,
) -> WidgetResult<LapTimerLayout> {
    let mut style = validated_lap_timer_style(config, scene);
    let label_style = ResolvedTextStyle {
        font_name: Some(config.label_font_name.clone()),
        font_size: config.label_font_size * scene.scale,
        line_height: config.label_font_size * scene.scale * LINE_HEIGHT_RATIO,
        color: rgba_color(config.label_color, 1.0),
        ..style.clone()
    };
    let value_font = resolve_font(fonts, style.font_name.as_deref(), style.font_size)?;
    let label_font = resolve_font(
        fonts,
        label_style.font_name.as_deref(),
        label_style.font_size,
    )?;
    let data_top = style.y + label_style.line_height + style.font_size * LOG_ROW_GAP_RATIO;
    let row_stride = style.font_size * (LINE_HEIGHT_RATIO + LOG_ROW_GAP_RATIO);
    let mut layout = LapTimerLayout {
        config: config.clone(),
        style: style.clone(),
        value_font,
        label: None,
        best_values: BTreeMap::new(),
        headers: Vec::new(),
        completed_rows: Vec::new(),
        column_rights: [0.0; 3],
        data_top,
        row_stride,
        static_layer: Arc::new(Mutex::new(None)),
    };
    if config.mode == LapTimerMode::LapLog {
        layout.column_rights = log_column_rights(&style, history, &layout.value_font, &label_font);
        let header_style = scaled_opacity_style(label_style, LOG_HEADER_OPACITY);
        for (text, right) in ["LAP", "TIME", "DELTA"]
            .into_iter()
            .zip(layout.column_rights)
        {
            layout.headers.push(TextRun::cell(
                text,
                right,
                style.y,
                &header_style,
                &label_font,
            )?);
        }
        for (index, duration) in history.durations.iter().enumerate() {
            layout.completed_rows.push([
                TextRun::cell(
                    &history
                        .display_number(index, config.renumber_laps)
                        .to_string(),
                    layout.column_rights[0],
                    data_top,
                    &style,
                    &layout.value_font,
                )?,
                TextRun::cell(
                    &format_lap_duration(*duration),
                    layout.column_rights[1],
                    data_top,
                    &style,
                    &layout.value_font,
                )?,
            ]);
        }
    } else {
        let label = config.label.to_uppercase();
        if config.show_label {
            if !label.trim().is_empty() {
                layout.label = Some(TextRun::new(
                    &label,
                    &label,
                    label_style.clone(),
                    &label_font,
                )?);
            }
            style.y += label_style.line_height;
        }
        layout.style = style.clone();
        if config.mode == LapTimerMode::BestLap {
            let enabled = history
                .durations
                .iter()
                .enumerate()
                .filter(|(index, _)| !history.is_disabled(*index));
            let mut best = f64::INFINITY;
            for (_, duration) in enabled {
                best = best.min(*duration);
                let text = format_lap_duration(best);
                if !layout.best_values.contains_key(&text) {
                    let run = TextRun::new(
                        &text,
                        TABLE_VERTICAL_METRICS_TEXT,
                        style.clone(),
                        &layout.value_font,
                    )?;
                    layout.best_values.insert(text, run);
                }
            }
        }
    }
    Ok(layout)
}

impl LapTimerLayout {
    /// The frame drawn from glyphs directly, which layout also measures.
    pub fn prepare_frame(
        &self,
        history: LapHistory<'_>,
        content: &LapTimerContent,
    ) -> WidgetResult<LapTimerFrame> {
        let mut runs = self.static_runs(history, content)?;
        runs.extend(self.dynamic_runs(content)?);
        Ok(LapTimerFrame::new(runs, None, self.config.x)?
            .shown(content != &LapTimerContent::Hidden))
    }

    /// Only the current time/delta repaint while completed history and the label stay cached.
    pub fn prepare_cached_frame(
        &self,
        history: LapHistory<'_>,
        content: &LapTimerContent,
        raster_scale: PreviewRasterScale,
    ) -> WidgetResult<LapTimerFrame> {
        let key = match content {
            LapTimerContent::Value { .. } | LapTimerContent::Hidden => StaticContent::Label,
            LapTimerContent::Log {
                completed_lap_count,
                current_row,
            } => StaticContent::Log {
                completed: *completed_lap_count,
                current_number: current_row.as_ref().map(|(cells, _)| cells[0].clone()),
            },
        };
        let background = {
            let mut cache = self
                .static_layer
                .lock()
                .expect("lap timer cache lock poisoned");
            if cache.as_ref().map(|cache| &cache.content) != Some(&key) {
                let runs = self.static_runs(history, content)?;
                let layer = if runs.is_empty() {
                    None
                } else {
                    Some(Arc::new(TextLayer::new(runs, self.config.x, raster_scale)?))
                };
                *cache = Some(CachedStatic {
                    content: key,
                    layer,
                });
            }
            cache
                .as_ref()
                .expect("static lap state prepared")
                .layer
                .clone()
        };
        Ok(
            LapTimerFrame::new(self.dynamic_runs(content)?, background, self.config.x)?
                .shown(content != &LapTimerContent::Hidden),
        )
    }

    fn static_runs(
        &self,
        history: LapHistory<'_>,
        content: &LapTimerContent,
    ) -> WidgetResult<Vec<TextRun>> {
        let mut runs = Vec::new();
        match content {
            LapTimerContent::Value { .. } | LapTimerContent::Hidden => {
                if let Some(label) = &self.label {
                    runs.push(label.clone());
                }
            }
            LapTimerContent::Log {
                completed_lap_count,
                current_row,
            } => {
                runs.extend(self.headers.iter().cloned());
                let rows = self
                    .completed_rows
                    .get(..*completed_lap_count)
                    .ok_or_else(|| {
                        WidgetError::Render("lap log is missing completed rows".into())
                    })?;
                let best = best_completed_lap(history, *completed_lap_count)?;
                let offset = if current_row.is_some() {
                    self.row_stride
                } else {
                    0.0
                };
                let enabled_rows = (0..rows.len())
                    .rev()
                    .filter(|lap_index| !history.is_disabled(*lap_index));
                for (display_index, lap_index) in enabled_rows.enumerate() {
                    let y = offset + display_index as f32 * self.row_stride;
                    runs.extend(rows[lap_index].iter().cloned().map(|run| run.offset_y(y)));
                    let delta = best.map(|best| history.durations[lap_index] - best);
                    runs.push(self.delta_cell(
                        &format_lap_delta(delta),
                        delta_color(
                            self.config.positive_delta_color,
                            self.config.negative_delta_color,
                            delta,
                        ),
                        self.data_top + y,
                    )?);
                }
                if let Some((cells, _)) = current_row {
                    runs.push(TextRun::cell(
                        &cells[0],
                        self.column_rights[0],
                        self.data_top,
                        &self.style,
                        &self.value_font,
                    )?);
                }
            }
        }
        Ok(runs)
    }

    fn dynamic_runs(&self, content: &LapTimerContent) -> WidgetResult<Vec<TextRun>> {
        let mut runs = Vec::new();
        match content {
            LapTimerContent::Hidden => runs.push(TextRun::new(
                PLACEHOLDER,
                TABLE_VERTICAL_METRICS_TEXT,
                self.style.clone(),
                &self.value_font,
            )?),
            LapTimerContent::Value { text, color } => {
                runs.push(if let Some(value) = self.best_values.get(text) {
                    value.clone()
                } else {
                    TextRun::new(
                        text,
                        TABLE_VERTICAL_METRICS_TEXT,
                        ResolvedTextStyle {
                            color: rgba_color(*color, 1.0),
                            ..self.style.clone()
                        },
                        &self.value_font,
                    )?
                });
            }
            LapTimerContent::Log { current_row, .. } => {
                if let Some((cells, color)) = current_row {
                    for (index, text) in cells.iter().enumerate().skip(1) {
                        runs.push(TextRun::cell(
                            text,
                            self.column_rights[index],
                            self.data_top,
                            &ResolvedTextStyle {
                                color: if index == 2 {
                                    rgba_color(*color, 1.0)
                                } else {
                                    self.style.color
                                },
                                ..self.style.clone()
                            },
                            &self.value_font,
                        )?);
                    }
                }
            }
        }
        Ok(runs)
    }

    fn delta_cell(&self, text: &str, color: [u8; 4], top: f32) -> WidgetResult<TextRun> {
        TextRun::cell(
            text,
            self.column_rights[2],
            top,
            &ResolvedTextStyle {
                color: rgba_color(color, 1.0),
                ..self.style.clone()
            },
            &self.value_font,
        )
    }
}

#[expect(
    clippy::redundant_closure_call,
    reason = "clippy counts only the direct call to `scaled`, not its `.map(scaled)` uses"
)]
fn scaled_opacity_style(mut style: ResolvedTextStyle, multiplier: f32) -> ResolvedTextStyle {
    let scaled = |color: skia_safe::Color| {
        skia_safe::Color::from_argb(
            ((color.a() as f32) * multiplier).round() as u8,
            color.r(),
            color.g(),
            color.b(),
        )
    };
    style.color = scaled(style.color);
    style.shadow_color = style.shadow_color.map(scaled);
    style.border_color = style.border_color.map(scaled);
    style.opacity *= multiplier;
    style
}

fn log_column_rights(
    style: &ResolvedTextStyle,
    history: LapHistory<'_>,
    row_font: &Font,
    header_font: &Font,
) -> [f32; 3] {
    let width = |text: &str, font: &Font| measure_text_with_font(text, font, style.numerals).width;
    let character_width = |character: char| {
        let mut buffer = [0; 4];
        width(character.encode_utf8(&mut buffer), row_font)
    };
    let widest_digit = ('0'..='9').map(&character_width).fold(0.0, f32::max);
    let widest_sign = character_width('+').max(character_width('-'));
    let completed = history.durations.iter().copied().fold(0.0, f64::max);
    let current = history
        .starts
        .partition_point(|start| *start <= history.end)
        .checked_sub(1)
        .map_or(0.0, |index| history.end - history.starts[index]);
    let hundredths = (completed.max(current) * 100.0).round() as u64;
    let hours = hundredths >= 360_000;
    let hour_digits = if hours {
        digit_count(hundredths / 360_000).max(2)
    } else {
        0
    };
    let lap = width("LAP", header_font)
        .max(digit_count(history.durations.len() as u64 + 1) as f32 * widest_digit);
    let time = width("TIME", header_font).max(
        (hour_digits + 6) as f32 * widest_digit
            + (if hours { 2.0 } else { 1.0 }) * character_width(':')
            + character_width('.'),
    );
    let delta = width("DELTA", header_font).max(
        widest_sign
            + (digit_count(hundredths / 100) + 2) as f32 * widest_digit
            + character_width('.'),
    );
    let gap = style.font_size * LOG_COLUMN_GAP_RATIO;
    let lap_right = style.x + lap;
    let time_right = lap_right + gap + time;
    [lap_right, time_right, time_right + gap + delta]
}

fn digit_count(value: u64) -> usize {
    value.max(1).ilog10() as usize + 1
}

/// One prepared table cell; split tables share lap-log typography.
#[derive(Clone, Debug)]
pub(crate) struct TextRun {
    blob: TextBlob,
    style: ResolvedTextStyle,
    baseline: f32,
    pub content: Rect,
    pub bounds: Rect,
    pub right: f32,
}

impl TextRun {
    pub fn new(
        text: &str,
        metrics_text: &str,
        style: ResolvedTextStyle,
        font: &Font,
    ) -> WidgetResult<Self> {
        let layout = layout_text(text, font, 0.0, style.numerals);
        let measurement = layout.measurement;
        let baseline =
            baseline_for_text_top_with_line_height(metrics_text, style.y, font, style.line_height);
        let mut content = Rect::new(
            style.x + measurement.bounds_left,
            baseline + measurement.bounds_top,
            style.x + measurement.bounds_right,
            baseline + measurement.bounds_bottom,
        );
        if style.border_color.is_some() {
            content.outset((style.border_thickness * 0.5, style.border_thickness * 0.5));
        }
        let mut bounds = content;
        content = text_selection_bounds(&measurement, style.x, baseline);
        if style.border_color.is_some() {
            content.outset((style.border_thickness * 0.5, style.border_thickness * 0.5));
        }
        let shadow = if style.shadow_color.is_some() && style.shadow_strength > 0.0 {
            style.shadow_distance.abs() + style.shadow_strength * 3.0
        } else {
            0.0
        };
        bounds.outset((shadow + 2.0, shadow + 2.0));
        Ok(Self {
            blob: layout
                .blob
                .ok_or_else(|| WidgetError::Render("unable to lay out lap timer text".into()))?,
            right: style.x + measurement.width,
            style,
            baseline,
            content,
            bounds,
        })
    }

    pub fn cell(
        text: &str,
        right: f32,
        top: f32,
        style: &ResolvedTextStyle,
        font: &Font,
    ) -> WidgetResult<Self> {
        let width = measure_text_with_font(text, font, style.numerals).width;
        Self::new(
            text,
            TABLE_VERTICAL_METRICS_TEXT,
            ResolvedTextStyle {
                x: right - width,
                y: top,
                line_height: style.font_size * LINE_HEIGHT_RATIO,
                ..style.clone()
            },
            font,
        )
    }

    pub fn offset_y(mut self, offset: f32) -> Self {
        self.style.y += offset;
        self.baseline += offset;
        self.content.offset((0.0, offset));
        self.bounds.offset((0.0, offset));
        self
    }

    pub fn draw(&self, canvas: &Canvas) {
        draw_text_blob(canvas, &self.blob, self.baseline, &self.style);
    }
}

#[derive(Clone, Debug)]
pub(super) struct TextLayer {
    raster: StaticRaster,
    content_bounds: Rect,
    content_width: f32,
}

impl TextLayer {
    pub fn new(runs: Vec<TextRun>, x: f32, raster_scale: PreviewRasterScale) -> WidgetResult<Self> {
        let frame = LapTimerFrame::new(runs, None, x)?;
        let raster = StaticRaster::new(
            raster_scale,
            Point::default(),
            frame.bounds,
            TEXT_COLOR_TYPE,
            |canvas| {
                frame.draw(canvas);
                Ok(())
            },
        )?;
        Ok(Self {
            raster,
            content_bounds: frame.content_bounds,
            content_width: frame.content_width,
        })
    }
}

pub struct LapTimerFrame {
    runs: Vec<TextRun>,
    background: Option<Arc<TextLayer>>,
    /// Hidden frames keep their layout bounds but draw nothing.
    shown: bool,
    pub content_bounds: Rect,
    pub bounds: Rect,
    pub content_width: f32,
}

impl LapTimerFrame {
    pub(super) fn new(
        runs: Vec<TextRun>,
        background: Option<Arc<TextLayer>>,
        x: f32,
    ) -> WidgetResult<Self> {
        let (mut content_bounds, mut bounds, mut content_width) = if let Some(layer) = &background {
            (
                layer.content_bounds,
                layer.raster.bounds(),
                layer.content_width,
            )
        } else {
            let first = runs
                .first()
                .ok_or_else(|| WidgetError::Render("lap timer has no drawable text".into()))?;
            (first.content, first.bounds, first.right - x)
        };
        for run in &runs {
            content_bounds.join(run.content);
            bounds.join(run.bounds);
            content_width = content_width.max(run.right - x);
        }
        bounds = Rect::new(
            bounds.left.floor(),
            bounds.top.floor(),
            bounds.right.ceil(),
            bounds.bottom.ceil(),
        );
        Ok(Self {
            runs,
            background,
            shown: true,
            content_bounds,
            bounds,
            content_width,
        })
    }

    pub fn shown(mut self, shown: bool) -> Self {
        self.shown = shown;
        self
    }

    pub fn draw(&self, canvas: &Canvas) {
        if !self.shown {
            return;
        }
        if let Some(layer) = &self.background {
            layer.raster.draw(canvas, None);
        }
        for run in &self.runs {
            run.draw(canvas);
        }
    }
}
