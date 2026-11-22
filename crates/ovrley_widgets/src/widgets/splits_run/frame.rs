//! One frame composites the cached rows and paints the running bar and pace.
use super::layout::SplitsLayout;
use crate::{
    render::{preview::StaticRaster, track_fill::TrackFill},
    widgets::lap_timer::frame::TextRun,
};
use skia_safe::{Canvas, Rect};
use std::sync::Arc;

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
