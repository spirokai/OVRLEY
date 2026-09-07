//! Plain numeric correspondences and tracking quality; no job or activity state.

pub(crate) mod opencv_pyrlk;

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct PointPair {
    pub before: Point,
    pub after: Point,
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct TrackingQuality {
    pub detected: usize,
    pub retained: usize,
    /// Fraction of occupied cells in a 4 × 4 image grid.
    pub coverage: f64,
    pub mean_forward_backward_error_pixels: f64,
}

pub(crate) struct TrackedPairs {
    pub pairs: Vec<PointPair>,
    pub quality: TrackingQuality,
    pub height_over_width: f64,
}

pub(crate) enum TrackingOutcome {
    Tracked(TrackedPairs),
    Insufficient(TrackingQuality),
}

pub(crate) fn coverage(points: impl Iterator<Item = Point>) -> f64 {
    let mut cells = [false; 16];
    for point in points {
        let x = ((point.x + 0.5) * 4.0).floor().clamp(0.0, 3.0) as usize;
        let y = ((point.y + 0.5) * 4.0).floor().clamp(0.0, 3.0) as usize;
        cells[y * 4 + x] = true;
    }
    cells.iter().filter(|cell| **cell).count() as f64 / 16.0
}
