//! Port of `features/player/utils/timelineGeometry.js` and `hooks/useTimelineClips.js` (plan D8):
//! the timeline's geometry and its lanes as a view model built in Rust. Slint only draws it.
//!
//! Positions are logical pixels from the timeline's left edge. Zoomed in, a clip can be millions of
//! pixels wide, which `f32` lengths cannot place exactly, so every box is cut to the visible
//! timeline (with a margin) here, and what sits inside a box is placed relative to the cut box.

use super::viewport::TimelineMetrics;
use crate::i18n::{catalog, interpolate, Locale};
use crate::lib::activity::ActivityWindow;
use crate::lib::file_dialog::filename_from_selected_path;
use crate::lib::format::format_clock_duration;
use crate::lib::video_program::collection::sort_clips_by_timeline;
use crate::lib::video_program::program::Overlap;
use crate::lib::video_program::segments::list_video_segments;
use crate::lib::video_program::warnings::{format_video_warning, VideoWarnings};
use crate::lib::video_program::Clip;
use crate::lib::widget::activity_attribute_label;
use crate::store::slices::editor::TimelineViewport;
use crate::store::slices::media::ActivitySummary;
use crate::store::slices::video_import::{ClipLane, SelectedClip};

/// `SNAP_THRESHOLD_PX`: snap distance on screen.
pub const SNAP_THRESHOLD_PX: f64 = 10.0;
/// `TEXT_HIDE_THRESHOLD_REM`, `EDGE_CUT_MIN_CLIP_REM` and `VIDEO_CLIP_HEIGHT_REM`.
const TEXT_HIDE_THRESHOLD_REM: f64 = 3.0;
const EDGE_CUT_MIN_CLIP_REM: f64 = 3.0;
pub const VIDEO_CLIP_HEIGHT_REM: f64 = 3.0;
/// Half the width of a cut marker's line (`w-1`).
const CUT_LINE_HALF_WIDTH_PX: f64 = 2.0;

/// `getClipGeometry`: a clip's x and width in the viewport, `None` when it is not visible.
pub fn clip_geometry(start_second: f64, duration_seconds: f64, view: TimelineViewport, width_px: f64) -> Option<(f64, f64)> {
    let span = view.view_end - view.view_start;
    let end = start_second + duration_seconds;
    if span <= 0.0 || width_px <= 0.0 || end <= view.view_start || start_second >= view.view_end || duration_seconds <= 0.0 {
        return None;
    }
    Some(((start_second - view.view_start) / span * width_px, duration_seconds / span * width_px))
}

/// `secondsToViewPx`.
pub fn seconds_to_view_px(second: f64, view: TimelineViewport, width_px: f64) -> f64 {
    let span = view.view_end - view.view_start;
    if span <= 0.0 || width_px <= 0.0 {
        return 0.0;
    }
    (second - view.view_start) / span * width_px
}

/// `snapClipDelta`'s result: the snapped shift and the edge it snapped to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Snap {
    pub delta: f64,
    pub guideline_second: Option<f64>,
}

/// `snapClipDelta`: a proposed shift snapped so one moving edge lands on a fixed edge, within
/// `threshold_px` on screen.
pub fn snap_clip_delta(proposed_delta: f64, moving_edges: &[f64], fixed_edges: &[f64], view: TimelineViewport, width_px: f64, threshold_px: f64) -> Snap {
    let span = view.view_end - view.view_start;
    let threshold = if span > 0.0 && width_px > 0.0 { threshold_px / width_px * span } else { 0.0 };
    let mut best = f64::INFINITY;
    let mut snap = Snap { delta: proposed_delta, guideline_second: None };
    for moving in moving_edges {
        for fixed in fixed_edges {
            let candidate = fixed - moving;
            let distance = (proposed_delta - candidate).abs();
            if distance <= threshold && distance < best {
                best = distance;
                snap = Snap { delta: candidate, guideline_second: Some(*fixed) };
            }
        }
    }
    snap
}

/// A horizontal span cut to the visible timeline: `(x, width, offset)`, where `offset` is how far
/// the cut box starts after the true one (what inside it shifts by).
fn visible_span(x: f64, width: f64, timeline_width: f64) -> (f64, f64, f64) {
    let margin = timeline_width.max(1.0);
    let start = x.max(-margin);
    let end = (x + width).min(timeline_width + margin);
    (start, (end - start).max(0.0), start - x)
}

/// Diagonal stripes as path commands over a `width × height` box, a line every `period` px measured
/// across the stripes: rising (`/`, CSS 135°) or falling (`\`, CSS 45°).
pub fn stripes_commands(width: f64, height: f64, period: f64, rising: bool) -> String {
    let step = period * std::f64::consts::SQRT_2;
    let mut commands = String::new();
    let mut c = -height;
    while c < width + step {
        if rising {
            commands.push_str(&format!("M {:.2} {:.2} L {:.2} 0 ", c, height, c + height));
        } else {
            commands.push_str(&format!("M {:.2} 0 L {:.2} {:.2} ", c, c + height, height));
        }
        c += step;
    }
    commands.trim_end().to_string()
}

/// The clip's tooltip, shown on hover above its visible part.
#[derive(Clone, Debug, PartialEq)]
pub struct TooltipView {
    /// The anchor's x in timeline pixels.
    pub anchor_x: f64,
    pub title: String,
    pub duration_label: String,
    /// The activity's metrics, sorted and joined.
    pub metrics: String,
    pub warnings: Vec<String>,
}

/// A clip on a lane.
#[derive(Clone, Debug, PartialEq)]
pub struct ClipView {
    pub lane: ClipLane,
    pub x: f64,
    pub width: f64,
    pub label: String,
    pub duration_label: String,
    /// The activity's format (`FIT`), or `DATA`.
    pub format_label: String,
    pub missing: bool,
    pub show_text: bool,
    /// The activity's text over its untrimmed part, relative to the clip box.
    pub text_x: f64,
    pub text_width: f64,
    /// A video clip's badge at the left of its visible part, relative to the clip box.
    pub badge_x: f64,
    pub badge_max_width: f64,
    /// The whole clip is selected (not one of its segments).
    pub selected: bool,
    pub has_selected_segment: bool,
    pub has_warnings: bool,
    pub tooltip: Option<TooltipView>,
}

/// A segment of a video clip, in timeline pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct SegmentView {
    pub video_id: String,
    pub key: String,
    pub x: f64,
    pub width: f64,
    pub excluded: bool,
    pub missing: bool,
    pub seam_before: bool,
    pub seam_after: bool,
    pub selected: bool,
    /// The stripes of an excluded segment, over its box.
    pub stripes: String,
    /// How far the box starts after the segment's true left edge (filmstrip tiles shift by it).
    pub offset: f64,
}

/// A striped span (an activity trim mask, an overlap band), in timeline pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct BandView {
    pub x: f64,
    pub width: f64,
    pub stripes: String,
}

/// Where a marker's hit area and line sit against its edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkerAlign {
    Start,
    Center,
    End,
}

/// What a marker moves.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MarkerTarget {
    /// A cut of a video, or an edge cut (`edge-start`, `edge-end`).
    Cut { video_id: String, cut_id: String },
    /// An edge of the activity trim.
    ActivityTrim(crate::lib::activity::TrimEdge),
}

/// A cut marker or an activity trim handle.
#[derive(Clone, Debug, PartialEq)]
pub struct MarkerView {
    pub target: MarkerTarget,
    /// The marker's second in timeline pixels.
    pub x: f64,
    pub align: MarkerAlign,
    /// The line sits inside the clip, at the timeline's own edge.
    pub line_inside: bool,
    pub label: String,
    pub dragging: bool,
    /// A real cut: its click opens "Remove cut".
    pub removable: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct VideoLaneView {
    pub clips: Vec<ClipView>,
    pub segments: Vec<SegmentView>,
    pub bands: Vec<BandView>,
    pub cut_markers: Vec<MarkerView>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ActivityLaneView {
    pub clip: Option<ClipView>,
    pub masks: Vec<BandView>,
    pub trim_markers: Vec<MarkerView>,
}

/// The lanes' inputs.
pub struct LaneInputs<'a> {
    pub view: TimelineViewport,
    pub metrics: TimelineMetrics,
    pub locale: Locale,
    /// Displayed clips (with a dragged cut's draft).
    pub clips: &'a [Clip],
    pub overlaps: &'a [Overlap],
    pub warnings: &'a VideoWarnings,
    pub selected: Option<&'a SelectedClip>,
    pub hovered: Option<&'a ClipLane>,
    /// `hasActivityData && playback.hasActivity`.
    pub has_activity: bool,
    pub activity_summary: Option<&'a ActivitySummary>,
    pub activity_filename: Option<String>,
    /// The displayed trim window and its dragged edge, or `None` where the trim is not edited
    /// (video sync).
    pub activity_trim: Option<(ActivityWindow, Option<crate::lib::activity::TrimEdge>)>,
    /// The cut markers show only in the editing workspace.
    pub show_cut_markers: bool,
    /// The cut being dragged: `(video id, cut id)`.
    pub dragged_cut: Option<(&'a str, &'a str)>,
}

fn text(locale: Locale, key: &str, default: &'static str) -> String {
    catalog().t_or(locale, key, default).to_string()
}

/// `isEdgeLineInside`.
fn edge_line_inside(align: MarkerAlign, px: f64, width_px: f64) -> bool {
    match align {
        MarkerAlign::Start => px < CUT_LINE_HALF_WIDTH_PX,
        MarkerAlign::End => px > width_px - CUT_LINE_HALF_WIDTH_PX,
        MarkerAlign::Center => false,
    }
}

struct ClipInput {
    lane: ClipLane,
    start_second: f64,
    duration_seconds: f64,
    label: String,
    format_label: String,
    missing: bool,
    is_video: bool,
    warnings: Vec<String>,
    metrics: Option<String>,
}

fn clip_view(input: ClipInput, inputs: &LaneInputs<'_>) -> Option<ClipView> {
    let width_px = inputs.metrics.width_px;
    let (x, width) = clip_geometry(input.start_second, input.duration_seconds, inputs.view, width_px)?;
    let visible_start = x.clamp(0.0, width_px);
    let visible_end = (x + width).clamp(0.0, width_px);
    let (box_x, box_width, offset) = visible_span(x, width, width_px);
    let selection = inputs.selected.filter(|selection| selection.lane == input.lane);
    let selected_segment = selection.and_then(|selection| selection.segment_key.as_ref());
    let window = match (&inputs.activity_trim, input.is_video) {
        (Some((window, _)), false) if window.is_trimmed && input.duration_seconds > 0.0 => (window.start_second, window.end_second),
        _ => (0.0, input.duration_seconds),
    };
    let text_true_x = window.0 / input.duration_seconds * width;
    let text_width = (window.1 - window.0) / input.duration_seconds * width;
    let tooltip = (inputs.hovered == Some(&input.lane) && !input.label.is_empty()).then(|| TooltipView {
        anchor_x: visible_start + (visible_end - visible_start).max(0.0) / 2.0,
        title: input.label.clone(),
        duration_label: format_clock_duration(input.duration_seconds),
        metrics: input.metrics.clone().unwrap_or_default(),
        warnings: input.warnings.clone(),
    });
    Some(ClipView {
        x: box_x,
        width: box_width,
        duration_label: format_clock_duration(input.duration_seconds),
        format_label: input.format_label,
        missing: input.missing,
        show_text: text_width >= TEXT_HIDE_THRESHOLD_REM * inputs.metrics.rem,
        text_x: text_true_x - offset,
        text_width,
        badge_x: visible_start - box_x,
        badge_max_width: visible_end - visible_start,
        selected: selection.is_some() && selected_segment.is_none(),
        has_selected_segment: selected_segment.is_some(),
        has_warnings: !input.warnings.is_empty(),
        tooltip,
        lane: input.lane,
        label: input.label,
    })
}

fn segment_views(clip: &Clip, inputs: &LaneInputs<'_>) -> Vec<SegmentView> {
    let width_px = inputs.metrics.width_px;
    let segments = list_video_segments(clip);
    let selected_key = inputs.selected.filter(|selection| selection.lane.video_id() == Some(clip.id())).and_then(|selection| selection.segment_key.as_deref());
    let height = VIDEO_CLIP_HEIGHT_REM * inputs.metrics.rem;
    let count = segments.len();
    segments
        .into_iter()
        .enumerate()
        .filter_map(|(index, segment)| {
            let start = clip.offset_seconds() + clip.fps().frames_to_seconds(segment.start_frame);
            let end = clip.offset_seconds() + clip.fps().frames_to_seconds(segment.end_frame);
            let x = seconds_to_view_px(start, inputs.view, width_px);
            let width = seconds_to_view_px(end, inputs.view, width_px) - x;
            if x + width < -width_px || x > 2.0 * width_px || width <= 0.0 {
                return None;
            }
            let (box_x, box_width, offset) = visible_span(x, width, width_px);
            Some(SegmentView {
                video_id: clip.id().to_string(),
                excluded: segment.edits.excluded,
                stripes: if segment.edits.excluded { stripes_commands(box_width, height, 3.0, false) } else { String::new() },
                selected: selected_key == Some(segment.key.as_str()),
                key: segment.key,
                x: box_x,
                width: box_width,
                missing: clip.video.missing,
                seam_before: index > 0,
                seam_after: index + 1 < count,
                offset,
            })
        })
        .collect()
}

/// `activityTrimMasks`: stripes over the visible trimmed-off parts.
fn activity_masks(window: &ActivityWindow, duration_seconds: f64, inputs: &LaneInputs<'_>) -> Vec<BandView> {
    if !window.is_trimmed || duration_seconds <= 0.0 {
        return Vec::new();
    }
    let height = 1.5 * inputs.metrics.rem;
    [(0.0, window.start_second), (window.end_second, duration_seconds)]
        .into_iter()
        .map(|(from, to)| (from.max(inputs.view.view_start), to.min(inputs.view.view_end)))
        .filter(|(from, to)| to > from)
        .map(|(from, to)| {
            let x = seconds_to_view_px(from, inputs.view, inputs.metrics.width_px);
            let width = seconds_to_view_px(to, inputs.view, inputs.metrics.width_px) - x;
            BandView { x, width, stripes: stripes_commands(width, height, 3.0, false) }
        })
        .collect()
}

/// The edge cuts of a clip (`listEdgeCuts`): one per end whose outer part is still exported.
pub fn edge_cuts(clip: &Clip) -> Vec<crate::lib::video_program::Cut> {
    use crate::lib::video_program::{Cut, FIRST_SEGMENT_KEY};
    let last_key = clip.cuts().last().map_or(FIRST_SEGMENT_KEY, |cut| cut.id.as_str());
    let mut cuts = Vec::new();
    if !clip.segment(FIRST_SEGMENT_KEY).excluded {
        cuts.push(Cut { id: EDGE_START.into(), frame: 0 });
    }
    if !clip.segment(last_key).excluded {
        cuts.push(Cut { id: EDGE_END.into(), frame: clip.frame_count() });
    }
    cuts
}

/// The ids of a clip's edge cuts.
pub const EDGE_START: &str = "edge-start";
pub const EDGE_END: &str = "edge-end";

fn cut_markers(clips: &[&Clip], inputs: &LaneInputs<'_>) -> Vec<MarkerView> {
    let width_px = inputs.metrics.width_px;
    let min_clip_px = EDGE_CUT_MIN_CLIP_REM * inputs.metrics.rem;
    let mut markers = Vec::new();
    for clip in clips {
        let clip_width = clip_geometry(clip.offset_seconds(), clip.duration_seconds(), inputs.view, width_px).map_or(0.0, |(_, width)| width);
        if clip_width < min_clip_px {
            continue;
        }
        let edges = edge_cuts(clip);
        let name = filename_from_selected_path(&clip.video.path).unwrap_or_default().to_string();
        // A fixed order (start edge, cuts, end edge) keeps a dragged edge cut's element, and its grab.
        let ordered = edges.iter().filter(|cut| cut.id == EDGE_START).chain(clip.cuts().iter()).chain(edges.iter().filter(|cut| cut.id == EDGE_END));
        for cut in ordered {
            let source_second = clip.fps().frames_to_seconds(cut.frame);
            let second = clip.offset_seconds() + source_second;
            if second < inputs.view.view_start || second > inputs.view.view_end {
                continue;
            }
            let px = seconds_to_view_px(second, inputs.view, width_px);
            let align = if cut.frame == 0 {
                MarkerAlign::Start
            } else if cut.frame == clip.frame_count() {
                MarkerAlign::End
            } else {
                MarkerAlign::Center
            };
            let label = match cut.id.as_str() {
                EDGE_START => interpolate(&text(inputs.locale, "player.trimStart", "Drag to trim the start of {{name}}"), &[("name", &name)]),
                EDGE_END => interpolate(&text(inputs.locale, "player.trimEnd", "Drag to trim the end of {{name}}"), &[("name", &name)]),
                _ => interpolate(&text(inputs.locale, "player.cutAt", "Cut at {{time}}"), &[("time", &format_clock_duration(source_second))]),
            };
            markers.push(MarkerView {
                target: MarkerTarget::Cut { video_id: clip.id().to_string(), cut_id: cut.id.clone() },
                x: px,
                align,
                line_inside: edge_line_inside(align, px, width_px),
                label,
                dragging: inputs.dragged_cut == Some((clip.id(), cut.id.as_str())),
                removable: cut.id != EDGE_START && cut.id != EDGE_END,
            });
        }
    }
    markers
}

/// The video lane, when there are clips.
pub fn video_lane(inputs: &LaneInputs<'_>) -> Option<VideoLaneView> {
    if inputs.clips.is_empty() {
        return None;
    }
    let sorted = sort_clips_by_timeline(inputs.clips);
    let mut lane = VideoLaneView::default();
    for clip in &sorted {
        let warnings = inputs.warnings.get(clip.id()).map(|warnings| warnings.iter().map(|warning| format_video_warning(warning, inputs.locale)).collect()).unwrap_or_default();
        let input = ClipInput {
            lane: ClipLane::Video(clip.id().to_string()),
            start_second: clip.offset_seconds(),
            duration_seconds: clip.duration_seconds(),
            label: filename_from_selected_path(&clip.video.path).unwrap_or_default().to_string(),
            format_label: "MP4".into(),
            missing: clip.video.missing,
            is_video: true,
            warnings,
            metrics: None,
        };
        if let Some(view) = clip_view(input, inputs) {
            lane.clips.push(view);
            lane.segments.extend(segment_views(clip, inputs));
        }
    }
    let band_height = VIDEO_CLIP_HEIGHT_REM * inputs.metrics.rem;
    lane.bands = inputs
        .overlaps
        .iter()
        .filter_map(|overlap| clip_geometry(overlap.start_second, overlap.end_second - overlap.start_second, inputs.view, inputs.metrics.width_px))
        .map(|(x, width)| {
            let (x, width, _) = visible_span(x, width, inputs.metrics.width_px);
            BandView { x, width, stripes: stripes_commands(width, band_height, 4.0, true) }
        })
        .collect();
    if inputs.show_cut_markers {
        lane.cut_markers = cut_markers(&sorted, inputs);
    }
    Some(lane)
}

/// The activity lane, while there is an activity to play.
pub fn activity_lane(inputs: &LaneInputs<'_>) -> Option<ActivityLaneView> {
    if !inputs.has_activity {
        return None;
    }
    let summary = inputs.activity_summary?;
    let duration = summary.duration_seconds;
    let mut metrics: Vec<String> = summary.available_metrics.iter().map(|metric| activity_attribute_label(&metric.attribute, inputs.locale)).collect();
    metrics.sort_by(|left, right| crate::lib::format::compare_text(left, right, inputs.locale));
    let input = ClipInput {
        lane: ClipLane::Activity,
        start_second: 0.0,
        duration_seconds: duration,
        label: inputs.activity_filename.clone().or_else(|| summary.file_name.clone()).unwrap_or_else(|| "Activity".into()),
        format_label: summary.file_format.as_ref().map(|format| format.to_uppercase()).filter(|format| !format.is_empty()).unwrap_or_else(|| "DATA".into()),
        missing: false,
        is_video: false,
        warnings: Vec::new(),
        metrics: Some(metrics.join(", ")),
    };
    let clip = clip_view(input, inputs);
    let Some((window, dragged)) = inputs.activity_trim else {
        return Some(ActivityLaneView { clip, masks: Vec::new(), trim_markers: Vec::new() });
    };
    let masks = activity_masks(&window, duration, inputs);
    let width_px = inputs.metrics.width_px;
    let clip_width = clip_geometry(0.0, duration, inputs.view, width_px).map_or(0.0, |(_, width)| width);
    let mut trim_markers = Vec::new();
    if clip_width >= EDGE_CUT_MIN_CLIP_REM * inputs.metrics.rem {
        use crate::lib::activity::TrimEdge;
        for edge in [TrimEdge::Start, TrimEdge::End] {
            let second = if edge == TrimEdge::Start { window.start_second } else { window.end_second };
            if second < inputs.view.view_start || second > inputs.view.view_end {
                continue;
            }
            let px = seconds_to_view_px(second, inputs.view, width_px);
            let align = if second <= 0.0 {
                MarkerAlign::Start
            } else if second >= duration {
                MarkerAlign::End
            } else {
                MarkerAlign::Center
            };
            let label = if edge == TrimEdge::Start {
                text(inputs.locale, "player.trimActivityStart", "Drag to trim the start of the activity")
            } else {
                text(inputs.locale, "player.trimActivityEnd", "Drag to trim the end of the activity")
            };
            trim_markers.push(MarkerView {
                target: MarkerTarget::ActivityTrim(edge),
                x: px,
                align,
                line_inside: edge_line_inside(align, px, width_px),
                label,
                dragging: dragged == Some(edge),
                removable: false,
            });
        }
    }
    Some(ActivityLaneView { clip, masks, trim_markers })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lib::activity::TrimEdge;
    use crate::lib::video_program::collection::join_video_edits;
    use crate::lib::video_program::fixtures::*;
    use crate::store::slices::media::{AvailableMetric, MetricSource};
    use ovrley_core::activity::schema::ParsedActivity;

    fn view(view_start: f64, view_end: f64) -> TimelineViewport {
        TimelineViewport { view_start, view_end }
    }

    #[test]
    fn maps_seconds_and_clips_to_pixels() {
        assert_eq!(seconds_to_view_px(50.0, view(0.0, 100.0), 500.0), 250.0);
        assert_eq!(seconds_to_view_px(5.0, view(10.0, 10.0), 500.0), 0.0);
        assert_eq!(clip_geometry(10.0, 20.0, view(0.0, 100.0), 500.0), Some((50.0, 100.0)));
        assert_eq!(clip_geometry(110.0, 20.0, view(0.0, 100.0), 500.0), None);
    }

    #[test]
    fn snaps_edges_within_a_screen_threshold() {
        let viewport = view(0.0, 100.0);
        assert_eq!(snap_clip_delta(0.5, &[0.0, 20.0], &[0.0, 100.0], viewport, 500.0, SNAP_THRESHOLD_PX), Snap { delta: 0.0, guideline_second: Some(0.0) });
        assert_eq!(snap_clip_delta(2.5, &[0.0, 20.0], &[0.0, 100.0], viewport, 500.0, SNAP_THRESHOLD_PX), Snap { delta: 2.5, guideline_second: None });
        assert_eq!(snap_clip_delta(2.5, &[0.0, 20.0], &[0.0, 100.0], viewport, 500.0, 15.0), Snap { delta: 0.0, guideline_second: Some(0.0) });
        assert_eq!(snap_clip_delta(9.6, &[0.0, 20.0], &[0.0, 30.0, 10.2], viewport, 500.0, SNAP_THRESHOLD_PX), Snap { delta: 10.0, guideline_second: Some(30.0) });
    }

    struct Fixture {
        clips: Vec<Clip>,
        warnings: VideoWarnings,
        summary: ActivitySummary,
    }

    fn fixture() -> Fixture {
        let videos = [video(TestVideo { id: "video-1", path: "C:\\clips\\ride.mp4", duration_seconds: 20.0, ..TestVideo::default() })];
        let clips = join_video_edits(&videos, &edits_at(&videos, &[("video-1", 5.0)]));
        let summary = ActivitySummary {
            available_metrics: vec![
                AvailableMetric { attribute: "gps_coordinates".into(), source: MetricSource::Direct },
                AvailableMetric { attribute: "barometric_altitude".into(), source: MetricSource::Direct },
            ],
            duration_seconds: 100.0,
            file_format: Some("fit".into()),
            file_name: Some("activity.fit".into()),
            ..ActivitySummary::of(&ParsedActivity::default())
        };
        Fixture { clips, warnings: VideoWarnings::new(), summary }
    }

    fn inputs<'a>(fixture: &'a Fixture, viewport: TimelineViewport, trim: Option<(ActivityWindow, Option<TrimEdge>)>) -> LaneInputs<'a> {
        LaneInputs {
            view: viewport,
            metrics: TimelineMetrics { width_px: 500.0, x: 0.0, scale_factor: 1.0, rem: 16.0 },
            locale: crate::i18n::DEFAULT_LOCALE,
            clips: &fixture.clips,
            overlaps: &[],
            warnings: &fixture.warnings,
            selected: None,
            hovered: None,
            has_activity: true,
            activity_summary: Some(&fixture.summary),
            activity_filename: Some("ride.fit".into()),
            activity_trim: trim,
            show_cut_markers: true,
            dragged_cut: None,
        }
    }

    #[test]
    fn clips_show_text_and_badges_at_their_visible_part() {
        let fixture = fixture();
        let lane = video_lane(&inputs(&fixture, view(0.0, 100.0), None)).unwrap();
        assert!(lane.clips[0].show_text);
        let shifted = video_lane(&inputs(&fixture, view(10.0, 110.0), None)).unwrap();
        assert_eq!((shifted.clips[0].badge_x, shifted.clips[0].badge_max_width), (25.0, 75.0));
    }

    #[test]
    fn the_trimmed_off_activity_is_striped_with_a_handle_on_each_edge() {
        let fixture = fixture();
        let window = ActivityWindow { start_second: 10.0, end_second: 80.0, is_trimmed: true };
        let lane = activity_lane(&inputs(&fixture, view(0.0, 100.0), Some((window, Some(TrimEdge::End))))).unwrap();
        assert_eq!(lane.masks.iter().map(|mask| (mask.x, mask.width)).collect::<Vec<_>>(), [(0.0, 50.0), (400.0, 100.0)]);
        let clip = lane.clip.unwrap();
        assert_eq!((clip.text_x, clip.text_width), (50.0, 350.0), "the text sits over the untrimmed part");
        assert_eq!(lane.trim_markers.iter().map(|marker| (marker.align, marker.dragging, marker.removable)).collect::<Vec<_>>(), [(MarkerAlign::Center, false, false), (MarkerAlign::Center, true, false)]);

        let zoomed = activity_lane(&inputs(&fixture, view(5.0, 15.0), Some((window, None)))).unwrap();
        assert_eq!(zoomed.masks.iter().map(|mask| (mask.x, mask.width)).collect::<Vec<_>>(), [(0.0, 250.0)], "only the visible part");
    }

    #[test]
    fn an_untrimmed_activity_has_handles_inside_its_edges_and_none_without_the_trim_model() {
        let fixture = fixture();
        let whole = ActivityWindow { start_second: 0.0, end_second: 100.0, is_trimmed: false };
        let lane = activity_lane(&inputs(&fixture, view(0.0, 100.0), Some((whole, None)))).unwrap();
        assert!(lane.masks.is_empty());
        assert_eq!(lane.trim_markers.iter().map(|marker| marker.align).collect::<Vec<_>>(), [MarkerAlign::Start, MarkerAlign::End]);
        let untouched = activity_lane(&inputs(&fixture, view(0.0, 100.0), None)).unwrap();
        assert!(untouched.masks.is_empty() && untouched.trim_markers.is_empty());
        assert_eq!(lane.clip.unwrap().format_label, "FIT");
    }

    #[test]
    fn video_clips_have_edge_cut_markers_once_wide_enough() {
        let fixture = fixture();
        let lane = video_lane(&inputs(&fixture, view(0.0, 100.0), None)).unwrap();
        assert_eq!(lane.cut_markers.iter().map(|marker| (marker.align, marker.removable)).collect::<Vec<_>>(), [(MarkerAlign::Start, false), (MarkerAlign::End, false)]);
        let narrow = video_lane(&inputs(&fixture, view(0.0, 1000.0), None)).unwrap();
        assert!(narrow.cut_markers.is_empty(), "10 px is too narrow for markers");
    }

    #[test]
    fn stripes_cover_the_box() {
        assert_eq!(stripes_commands(6.0, 6.0, 3.0, false).matches('M').count(), 4);
        assert!(stripes_commands(10.0, 4.0, 4.0, true).starts_with("M -4.00 4.00 L 0.00 0"));
    }
}
