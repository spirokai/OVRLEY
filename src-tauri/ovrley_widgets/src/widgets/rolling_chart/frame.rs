//! One frame samples every series across the window ending at the playhead.
use super::layout::{normalized, cut_corners, sample_times, RollingChartCache};
use crate::activity::ActivitySampler;
use skia_safe::{Canvas, PathBuilder, Point};

pub struct SeriesFrame {
    /// Contiguous observed stretches; gaps in the telemetry break the line. Curved charts
    /// join the recorded samples, others the time grid.
    runs: Vec<Vec<Point>>,
    /// The playhead's value at the right edge, when observed.
    marker: Option<Point>,
}

pub struct RollingChartFrame {
    series: Vec<SeriesFrame>,
}

impl RollingChartCache {
    /// Without an activity, or outside a metric's coverage, the plot stays empty.
    pub fn prepare_frame(&self, activity: Option<&ActivitySampler>, second: f64) -> RollingChartFrame {
        let times = sample_times(second, self.window, self.step);
        let series = self
            .series
            .iter()
            .map(|style| {
                let Some((activity, range)) =
                    activity.and_then(|activity| activity.plot_range(style.metric).map(|range| (activity, range)))
                else {
                    return SeriesFrame { runs: Vec::new(), marker: None };
                };
                let point = |(time, value): (f64, f64)| Point::new(self.x(time, second), self.y(normalized(value, range)));
                let mut runs: Vec<Vec<(f64, f64)>> = Vec::new();
                let mut open = false;
                let mut last = None;
                for &time in &times {
                    let sample = activity.plot_value(style.metric, time).map(|value| (time, value));
                    match sample {
                        Some(sample) if open => runs.last_mut().expect("open run").push(sample),
                        Some(sample) => {
                            runs.push(vec![sample]);
                            open = true;
                        }
                        None => open = false,
                    }
                    last = sample;
                }
                let runs = runs
                    .into_iter()
                    .map(|run| {
                        let (first, end) = (run[0], run[run.len() - 1]);
                        // Corners exist only at recorded samples; denser data than the grid draws the grid.
                        let knots = (self.curve > 0.0 && run.len() > 1)
                            .then(|| activity.plot_knots(style.metric, first.0, end.0, run.len()))
                            .flatten();
                        match knots {
                            Some(knots) => [first].into_iter().chain(knots).chain([end]).map(point).collect(),
                            None => run.into_iter().map(point).collect(),
                        }
                    })
                    .collect();
                SeriesFrame { runs, marker: last.map(point) }
            })
            .collect();
        RollingChartFrame { series }
    }
}

impl RollingChartFrame {
    /// Series paint back to front, each with its area, line and marker.
    pub fn draw(&self, canvas: &Canvas, cache: &RollingChartCache) {
        for (style, frame) in cache.series.iter().zip(&self.series) {
            for run in &frame.runs {
                let mut line = PathBuilder::new();
                line.move_to(run[0]);
                cut_corners(&mut line, run, cache.curve);
                if run.len() == 1 {
                    // A lone sample still paints a round dot.
                    line.line_to(run[0]);
                }
                let line = line.detach();
                if let Some(area) = &style.area {
                    let (first, last) = (run[0], run[run.len() - 1]);
                    let mut fill = PathBuilder::new();
                    fill.move_to((first.x, cache.plot.bottom));
                    fill.line_to(first);
                    cut_corners(&mut fill, run, cache.curve);
                    fill.line_to((last.x, cache.plot.bottom));
                    fill.close();
                    canvas.draw_path(&fill.detach(), area);
                }
                canvas.draw_path(&line, &style.line);
            }
            if let (Some(marker), Some(point)) = (&style.marker, frame.marker) {
                marker.draw(canvas, point);
            }
        }
        if let Some(legend) = &cache.legend {
            legend.draw(canvas, None);
        }
    }
}
