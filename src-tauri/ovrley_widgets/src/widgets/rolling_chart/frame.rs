//! One frame samples every series across the window ending at the playhead.
use super::layout::{normalized, sample_times, RollingChartCache};
use crate::activity::ActivitySampler;
use skia_safe::{Canvas, PathBuilder, Point};

pub struct SeriesFrame {
    /// Contiguous observed stretches; gaps in the telemetry break the line.
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
                let mut runs: Vec<Vec<Point>> = Vec::new();
                let mut open = false;
                let mut last = None;
                let range = activity.and_then(|activity| activity.plot_range(style.metric));
                for &time in &times {
                    let point = activity.zip(range).and_then(|(activity, range)| {
                        activity.plot_value(style.metric, time).map(|value| {
                            Point::new(self.x(time, second), self.y(normalized(value, range)))
                        })
                    });
                    match point {
                        Some(point) if open => runs.last_mut().expect("open run").push(point),
                        Some(point) => {
                            runs.push(vec![point]);
                            open = true;
                        }
                        None => open = false,
                    }
                    last = point;
                }
                SeriesFrame { runs, marker: last }
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
                for point in &run[1..] {
                    line.line_to(*point);
                }
                if run.len() == 1 {
                    // A lone sample still paints a round dot.
                    line.line_to(run[0]);
                }
                let line = line.detach();
                if let Some(area) = &style.area {
                    let (first, last) = (run[0], run[run.len() - 1]);
                    let mut fill = PathBuilder::new();
                    fill.move_to((first.x, cache.plot.bottom));
                    for point in run {
                        fill.line_to(*point);
                    }
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
