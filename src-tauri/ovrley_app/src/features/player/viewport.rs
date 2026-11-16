//! Port of `features/player/utils/timelineViewport.js` and `hooks/useTimelineViewport.js` (plan D7):
//! fit targets, zoom, pan, playback follow and the tick strip, and the viewport's reconcile rules.
//!
//! The viewport lives in the store (`EditorSlice::timeline_viewport`). The measured width, the
//! device pixel grid and the rem are hook state, which `ovrley_ui` reports when they change.

use super::playback::{player_timing, timeline_minimum};
use crate::i18n::{catalog, Locale};
use crate::lib::format::format_clock_duration;
use crate::lib::utils::clamp;
use crate::lib::video_program::{Clip, SecondsRange};
use crate::store::slices::editor::{PreviewPlaybackState, TimelineViewport};
use crate::store::Store;

const VIEWPORT_MATCH_EPSILON_SECONDS: f64 = 0.001;
const ZOOM_FACTOR: f64 = 1.6;
const MIN_ZOOM_SPAN: f64 = 0.5;
const FIT_MIN_SPAN: f64 = 2.0;
const FIT_PADDING_RATIO: f64 = 0.04;
const TICK_TARGET_PX: f64 = 90.0;
const MAX_ZOOM_MAJOR_STEP_SECONDS: f64 = 2.0;
const FOLLOW_EDGE_PADDING_RATIO: f64 = 0.15;
const MINOR_TICKS_PER_STEP: usize = 5;
const NICE_STEPS: [f64; 18] = [0.1, 0.2, 0.5, 1.0, 2.0, 5.0, 10.0, 15.0, 30.0, 60.0, 120.0, 300.0, 600.0, 900.0, 1200.0, 1500.0, 1800.0, 3600.0];
/// `PX_PER_SECOND_PRECISION`: the scale is quantized so a pan keeps the tick strip.
const PX_PER_SECOND_PRECISION: f64 = 1e6;

fn viewport(view_start: f64, view_end: f64) -> TimelineViewport {
    TimelineViewport { view_start, view_end }
}

/// `rangesMatch`.
pub fn ranges_match(a: TimelineViewport, b: TimelineViewport) -> bool {
    (a.view_start - b.view_start).abs() <= VIEWPORT_MATCH_EPSILON_SECONDS && (a.view_end - b.view_end).abs() <= VIEWPORT_MATCH_EPSILON_SECONDS
}

/// `clampToView`: keeps a viewport inside `[minimum, total]`.
pub fn clamp_to_view(view_start: f64, view_end: f64, total_duration: f64, minimum: f64) -> TimelineViewport {
    let span = clamp(view_end - view_start, 0.0, total_duration - minimum);
    let start = clamp(view_start, minimum, total_duration - span);
    viewport(start, start + span)
}

/// `fitToFull`.
pub fn fit_to_full(total_duration: f64, minimum: f64) -> TimelineViewport {
    viewport(minimum, total_duration)
}

fn minimum_zoom_span(width_px: f64) -> f64 {
    if width_px <= 0.0 {
        return MIN_ZOOM_SPAN;
    }
    MIN_ZOOM_SPAN.max(width_px / TICK_TARGET_PX * MAX_ZOOM_MAJOR_STEP_SECONDS)
}

/// `getZoomLimits`: `(can zoom in, can zoom out)`.
pub fn zoom_limits(view: TimelineViewport, total_duration: f64, minimum: f64, width_px: f64) -> (bool, bool) {
    let bounds_span = total_duration - minimum;
    let span = view.view_end - view.view_start;
    let min_span = minimum_zoom_span(width_px).min(bounds_span);
    (span > min_span + VIEWPORT_MATCH_EPSILON_SECONDS, span < bounds_span - VIEWPORT_MATCH_EPSILON_SECONDS)
}

/// `zoomRange`: ×1.6 in (`zoom_in`) or out around `pivot`.
pub fn zoom_range(view: TimelineViewport, pivot: f64, zoom_in: bool, total_duration: f64, minimum: f64, width_px: f64) -> TimelineViewport {
    let bounds_span = total_duration - minimum;
    let span = view.view_end - view.view_start;
    let pivot = clamp(pivot, view.view_start, view.view_end);
    let ratio = if span > 0.0 { (pivot - view.view_start) / span } else { 0.5 };
    let min_span = minimum_zoom_span(width_px).min(bounds_span);
    let factor = if zoom_in { 1.0 / ZOOM_FACTOR } else { ZOOM_FACTOR };
    let new_span = clamp(span * factor, min_span, bounds_span);
    let mut start = pivot - ratio * new_span;
    let mut end = start + new_span;
    if start < minimum {
        start = minimum;
        end = minimum + new_span;
    }
    if end > total_duration {
        end = total_duration;
        start = total_duration - new_span;
    }
    viewport(start, end)
}

/// `fitRangeToViewport`: a range with 4% padding, at least the minimum span, inside the bounds.
pub fn fit_range_to_viewport(range_start: f64, range_end: f64, total_duration: f64, minimum: f64, width_px: f64) -> TimelineViewport {
    let bounds_span = total_duration - minimum;
    let start = clamp(range_start, minimum, total_duration);
    let end = clamp(start.max(range_end), start, total_duration);
    let padding = (end - start) * FIT_PADDING_RATIO;
    let (mut view_start, mut view_end) = (start - padding, end + padding);
    let mut span = view_end - view_start;
    let minimum_span = FIT_MIN_SPAN.max(minimum_zoom_span(width_px)).min(bounds_span);
    if span < minimum_span {
        let half_extra = (minimum_span - span) / 2.0;
        view_start -= half_extra;
        view_end += half_extra;
        span = minimum_span;
    }
    span = span.min(bounds_span);
    if view_start < minimum {
        view_start = minimum;
        view_end = minimum + span;
    }
    if view_end > total_duration {
        view_end = total_duration;
        view_start = total_duration - span;
    }
    viewport(view_start, view_end)
}

/// `panViewport`.
pub fn pan_viewport(view: TimelineViewport, delta_seconds: f64, total_duration: f64, minimum: f64) -> TimelineViewport {
    let bounds_span = total_duration - minimum;
    if bounds_span == 0.0 {
        return viewport(minimum, total_duration);
    }
    if view.view_end - view.view_start >= bounds_span {
        return view;
    }
    clamp_to_view(view.view_start + delta_seconds, view.view_end + delta_seconds, total_duration, minimum)
}

/// `followPlayhead`: keeps the playhead inside the middle 70% of a zoomed view.
pub fn follow_playhead(view: TimelineViewport, playhead: f64, total_duration: f64, minimum: f64) -> TimelineViewport {
    let span = view.view_end - view.view_start;
    if span <= 0.0 || span >= total_duration - minimum {
        return view;
    }
    let follow_start = view.view_start + FOLLOW_EDGE_PADDING_RATIO * span;
    let follow_end = view.view_end - FOLLOW_EDGE_PADDING_RATIO * span;
    if playhead >= follow_start && playhead <= follow_end {
        return view;
    }
    let offset_ratio = if playhead < follow_start { FOLLOW_EDGE_PADDING_RATIO } else { 1.0 - FOLLOW_EDGE_PADDING_RATIO };
    let start = playhead - offset_ratio * span;
    clamp_to_view(start, start + span, total_duration, minimum)
}

/// `resolveFitVideoClip`: the selected clip, else the clip at the playhead (the earliest where
/// clips overlap), else the nearest.
pub fn resolve_fit_video_clip<'a>(clips: &'a [Clip], selected_video_id: Option<&str>, second: f64) -> Option<&'a Clip> {
    if let Some(selected) = selected_video_id.and_then(|id| clips.iter().find(|clip| clip.id() == id)) {
        return Some(selected);
    }
    let distance = |clip: &Clip| 0.0_f64.max(clip.offset_seconds() - second).max(second - clip.end_second());
    clips.iter().fold(None, |nearest: Option<&Clip>, clip| match nearest {
        Some(best) if !(distance(clip) < distance(best) || (distance(clip) == distance(best) && clip.offset_seconds() < best.offset_seconds())) => Some(best),
        _ => Some(clip),
    })
}

/// A canonical fit target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FitTargetId {
    All,
    Video,
    Activity,
}

impl FitTargetId {
    pub fn id(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Video => "video",
            Self::Activity => "activity",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        [Self::All, Self::Video, Self::Activity].into_iter().find(|target| target.id() == id)
    }

    pub fn label(self, locale: Locale) -> String {
        let (key, default) = match self {
            Self::All => ("player.all", "All"),
            Self::Video => ("player.video", "Video"),
            Self::Activity => ("player.activity", "Activity"),
        };
        catalog().t_or(locale, key, default).to_string()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FitTarget {
    pub id: FitTargetId,
    pub viewport: TimelineViewport,
}

/// `buildFitTargets`: Video (one clip), Activity, and All unless one of them covers the timeline.
pub fn build_fit_targets(
    total_duration: f64,
    width_px: f64,
    video_bounds: Option<SecondsRange>,
    video_clip_range: Option<SecondsRange>,
    has_activity_data: bool,
    activity_duration_seconds: f64,
    fallback_duration_seconds: f64,
) -> Vec<FitTarget> {
    let minimum = timeline_minimum(video_bounds);
    let mut targets = Vec::new();
    let mut ranges = Vec::new();
    if let Some(range) = video_clip_range {
        ranges.push((range.start_second, range.end_second));
        targets.push(FitTarget { id: FitTargetId::Video, viewport: fit_range_to_viewport(range.start_second, range.end_second, total_duration, minimum, width_px) });
    }
    if has_activity_data {
        let duration = if activity_duration_seconds > 0.0 { activity_duration_seconds } else { fallback_duration_seconds };
        ranges.push((0.0, duration));
        targets.push(FitTarget { id: FitTargetId::Activity, viewport: fit_range_to_viewport(0.0, duration, total_duration, minimum, width_px) });
    }
    if !ranges.iter().any(|(start, end)| *start <= minimum && *end >= total_duration) {
        targets.insert(0, FitTarget { id: FitTargetId::All, viewport: fit_to_full(total_duration, minimum) });
    }
    targets
}

/// `getMatchingFitTargetId`.
pub fn matching_fit_target(view: TimelineViewport, targets: &[FitTarget]) -> Option<FitTargetId> {
    targets.iter().find(|target| ranges_match(view, target.viewport)).map(|target| target.id)
}

/// `getTimelineTickStep`: the smallest nice step that keeps labels 90 px apart.
pub fn timeline_tick_step(px_per_second: f64) -> f64 {
    let target = TICK_TARGET_PX / px_per_second;
    NICE_STEPS.into_iter().find(|step| *step >= target).unwrap_or(NICE_STEPS[NICE_STEPS.len() - 1])
}

/// `formatTickLabel`.
pub fn format_tick_label(second: f64, step: f64) -> String {
    if step < 1.0 {
        format!("{second:.1}s")
    } else {
        format_clock_duration(second)
    }
}

/// A tick, `x` from the strip's anchor.
#[derive(Clone, Debug, PartialEq)]
pub struct Tick {
    pub second: f64,
    pub x: f64,
    pub label: Option<String>,
}

/// `computeTimelineTickStrip`: major and minor ticks from a major tick over the width plus one
/// step, so panning within one step only moves the strip.
pub fn compute_tick_strip(anchor_second: f64, step: f64, px_per_second: f64, width_px: f64) -> (Vec<Tick>, Vec<Tick>) {
    let step_px = step * px_per_second;
    let major_count = (width_px / step_px).ceil() as usize + 1;
    let mut major = Vec::new();
    let mut minor = Vec::new();
    for index in 0..=major_count {
        let second = anchor_second + index as f64 * step;
        major.push(Tick { second, x: index as f64 * step_px, label: Some(format_tick_label(second, step)) });
        if index == major_count {
            break;
        }
        for minor_index in 1..MINOR_TICKS_PER_STEP {
            let offset = minor_index as f64 * step / MINOR_TICKS_PER_STEP as f64;
            minor.push(Tick { second: second + offset, x: (index as f64 * step + offset) * px_per_second, label: None });
        }
    }
    (major, minor)
}

/// The tick strip's placement at a viewport: its anchor, step and scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TickPlacement {
    pub px_per_second: f64,
    pub step: f64,
    pub anchor_second: f64,
}

/// The strip is rebuilt only when zoom, width or the major step under the viewport start changes.
pub fn tick_placement(view: TimelineViewport, width_px: f64) -> Option<TickPlacement> {
    let span = view.view_end - view.view_start;
    if span <= 0.0 || width_px <= 0.0 {
        return None;
    }
    let px_per_second = crate::js::round(width_px / span * PX_PER_SECOND_PRECISION) / PX_PER_SECOND_PRECISION;
    let step = timeline_tick_step(px_per_second);
    Some(TickPlacement { px_per_second, step, anchor_second: (view.view_start / step).floor() * step })
}

/// The measurements `ovrley_ui` reports.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TimelineMetrics {
    pub width_px: f64,
    /// The timeline's absolute x in the window, in logical pixels.
    pub x: f64,
    /// Screen pixels per logical pixel (the window's scale factor).
    pub scale_factor: f64,
    pub rem: f64,
}

impl Default for TimelineMetrics {
    fn default() -> Self {
        Self { width_px: 0.0, x: 0.0, scale_factor: 1.0, rem: 16.0 }
    }
}

impl TimelineMetrics {
    /// `snapToScreenPixel`: a timeline-relative x on a screen pixel edge.
    pub fn snap_to_screen_pixel(&self, value: f64) -> f64 {
        let origin = self.x * self.scale_factor;
        (crate::js::round(origin + value * self.scale_factor) - origin) / self.scale_factor
    }

    /// `roundToDevicePixel`.
    pub fn round_to_device_pixel(&self, value: f64) -> f64 {
        crate::js::round(value * self.scale_factor) / self.scale_factor
    }
}

/// `useTimelineViewport`'s state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ViewportState {
    pub metrics: TimelineMetrics,
    /// The media identity the viewport last reset for.
    media_identity: Option<String>,
    /// The bounds and drag flag the clamp rule last saw.
    clamp_inputs: Option<(bool, f64, f64)>,
    /// The follow rule's inputs.
    follow_inputs: Option<(bool, bool, f64)>,
}

/// The selected clip's video, for the Video fit target.
fn selected_video_id(store: &Store) -> Option<&str> {
    store.video_import.selected_clip.as_ref().and_then(|selection| selection.lane.video_id())
}

/// The fit targets of the store.
pub fn fit_targets(store: &Store) -> Vec<FitTarget> {
    let timing = player_timing(store);
    let committed = &store.player.timeline.timeline().committed;
    let clip_range = resolve_fit_video_clip(&committed.clips, selected_video_id(store), timing.clamped_playhead)
        .map(|clip| SecondsRange { start_second: clip.offset_seconds(), end_second: clip.end_second() });
    let summary = store.media.activity_summary.as_ref();
    build_fit_targets(
        timing.total_duration,
        store.player.viewport.metrics.width_px,
        committed.bounds,
        clip_range,
        summary.is_some(),
        summary.map_or(0.0, |summary| summary.duration_seconds),
        store.editor.fallback_duration_seconds,
    )
}

/// The minimum of the displayed timeline, which bounds the viewport during a drag.
fn displayed_minimum(store: &Store) -> f64 {
    timeline_minimum(store.player.timeline.timeline().displayed.bounds)
}

fn set_viewport(store: &mut Store, next: TimelineViewport) {
    // The pure helpers keep finite, ordered ranges; an empty timeline (0 s) keeps the last one.
    let _ = store.editor.set_timeline_viewport(next);
}

/// Toolbar zoom: ×1.6 around the playhead.
pub fn zoom(store: &mut Store, zoom_in: bool) {
    let pivot = player_timing(store).clamped_playhead;
    zoom_at(store, zoom_in, pivot);
}

/// Zoom around a pivot (Ctrl+wheel at the pointer).
pub fn zoom_at(store: &mut Store, zoom_in: bool, pivot: f64) {
    let timing = player_timing(store);
    let next = zoom_range(store.editor.timeline_viewport, pivot, zoom_in, timing.total_duration, displayed_minimum(store), store.player.viewport.metrics.width_px);
    set_viewport(store, next);
}

/// Reset view: the latest full timeline.
pub fn reset_view(store: &mut Store) {
    let next = fit_to_full(player_timing(store).total_duration, displayed_minimum(store));
    set_viewport(store, next);
}

/// A fit tab or shortcut. Returns whether the target exists.
pub fn fit_target(store: &mut Store, id: FitTargetId) -> bool {
    let Some(target) = fit_targets(store).into_iter().find(|target| target.id == id) else { return false };
    set_viewport(store, target.viewport);
    true
}

/// Pan by seconds (a lane-background drag, the plain wheel).
pub fn pan_by(store: &mut Store, delta_seconds: f64) {
    let next = pan_viewport(store.editor.timeline_viewport, delta_seconds, player_timing(store).total_duration, displayed_minimum(store));
    set_viewport(store, next);
}

/// `followSecond`: auto-scroll during a clip drag. Returns the change of the viewport's start, or
/// `None` when it did not move.
pub fn follow_second(store: &mut Store, second: f64, minimum: f64) -> Option<f64> {
    let previous = store.editor.timeline_viewport;
    let next = follow_playhead(previous, second, player_timing(store).total_duration, minimum);
    if next == previous {
        return None;
    }
    set_viewport(store, next);
    Some(next.view_start - previous.view_start)
}

/// The wheel over the timeline (`timeline.wheelPan`, `timeline.wheelZoom`): a plain wheel pans by
/// `delta_y / width × span`; Ctrl+wheel zooms around the pointer at `x` (timeline-relative).
pub fn wheel(store: &mut Store, delta_y: f64, x: f64, ctrl: bool) -> bool {
    use crate::lib::keyboard_shortcuts::{match_keyboard_shortcut, KeyInput, Scope};
    let input = KeyInput { key: "wheel".into(), ctrl, ..KeyInput::default() };
    let Some(found) = match_keyboard_shortcut(&input, Scope::Player) else { return false };
    let view = store.editor.timeline_viewport;
    let width = store.player.viewport.metrics.width_px;
    if width <= 0.0 {
        return false;
    }
    let span = view.view_end - view.view_start;
    match found.command.id.as_str() {
        "timeline.wheelPan" => pan_by(store, delta_y / width * span),
        "timeline.wheelZoom" => zoom_at(store, delta_y < 0.0, view.view_start + x / width * span),
        _ => return false,
    }
    true
}

/// The viewport rules of the `player` reconcile step, after the playback effects (D7).
pub fn reconcile_viewport(store: &mut Store, dragging: bool) {
    let timing = player_timing(store);
    let minimum = displayed_minimum(store);
    let summary = store.media.activity_summary.as_ref();
    let committed = &store.player.timeline.timeline().committed;
    let video_identity = committed.clips.iter().map(|clip| format!("{}:{}", clip.id(), clip.duration_seconds())).collect::<Vec<_>>().join("|");
    let identity = format!("{video_identity}|{}|{}|{}", summary.is_some(), summary.map_or(0.0, |summary| summary.duration_seconds), store.editor.fallback_duration_seconds);
    // 1. New media fits the full range, unless a project restored its viewport.
    if store.player.viewport.media_identity.as_deref() != Some(&identity) {
        store.player.viewport.media_identity = Some(identity);
        if store.editor.skip_next_timeline_viewport_reset {
            store.editor.skip_next_timeline_viewport_reset = false;
        } else {
            set_viewport(store, fit_to_full(timing.total_duration, minimum));
        }
    }
    // 2. A duration change clamps the viewport, except while dragging.
    let clamp_inputs = (dragging, minimum, timing.total_duration);
    if store.player.viewport.clamp_inputs != Some(clamp_inputs) {
        store.player.viewport.clamp_inputs = Some(clamp_inputs);
        if !dragging {
            let view = store.editor.timeline_viewport;
            set_viewport(store, clamp_to_view(view.view_start, view.view_end, timing.total_duration, minimum));
        }
    }
    // 3. While playing and not dragging, the viewport follows the playhead (not on the first run).
    let playing = store.editor.preview_playback_state == PreviewPlaybackState::Playing;
    let follow_inputs = (dragging, playing, timing.clamped_playhead);
    let previous = store.player.viewport.follow_inputs.replace(follow_inputs);
    if previous.is_some_and(|previous| previous != follow_inputs) && playing && !dragging {
        let next = follow_playhead(store.editor.timeline_viewport, timing.clamped_playhead, timing.total_duration, minimum);
        set_viewport(store, next);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lib::video_program::fixtures::*;

    fn range(start_second: f64, end_second: f64) -> Option<SecondsRange> {
        Some(SecondsRange { start_second, end_second })
    }

    #[test]
    fn zoom_limits_follow_the_span() {
        assert_eq!(zoom_limits(viewport(0.0, 100.0), 100.0, 0.0, 500.0), (true, false));
        assert_eq!(zoom_limits(viewport(20.0, 60.0), 100.0, 0.0, 500.0), (true, true));
        let mut view = viewport(0.0, 100.0);
        for _ in 0..20 {
            view = zoom_range(view, 50.0, true, 100.0, 0.0, 500.0);
        }
        assert_eq!(zoom_limits(view, 100.0, 0.0, 500.0), (false, true));
    }

    #[test]
    fn clamps_and_fits() {
        assert_eq!(clamp_to_view(150.0, 250.0, 200.0, 0.0), viewport(100.0, 200.0));
        assert_eq!(fit_to_full(2.509, 0.0), viewport(0.0, 2.509));
        assert_eq!(fit_to_full(25.0, -5.0), viewport(-5.0, 25.0));
        assert_eq!(clamp_to_view(-20.0, 0.0, 25.0, -5.0), viewport(-5.0, 15.0));
        let targets = build_fit_targets(25.0, 0.0, range(-5.0, 25.0), range(-5.0, 25.0), true, 25.0, 25.0);
        assert_eq!(targets[0].viewport, viewport(-5.0, 25.0));
    }

    #[test]
    fn zooms_around_a_pivot() {
        let result = zoom_range(viewport(0.0, 100.0), 50.0, true, 200.0, 0.0, 0.0);
        assert!((result.view_end - result.view_start - 100.0 / 1.6).abs() < 1e-9);
        assert!(result.view_start <= 50.0 && result.view_end >= 50.0);
    }

    #[test]
    fn fits_ranges_with_padding_and_a_minimum_span() {
        let result = fit_range_to_viewport(10.0, 90.0, 100.0, 0.0, 0.0);
        assert!((result.view_start - 6.8).abs() < 1e-9 && (result.view_end - 93.2).abs() < 1e-9);
        assert!(fit_range_to_viewport(50.0, 50.0, 100.0, 0.0, 0.0).view_end > 50.0);
    }

    #[test]
    fn pans_and_follows_inside_the_bounds() {
        assert_eq!(pan_viewport(viewport(0.0, 50.0), 10.0, 200.0, 0.0), viewport(10.0, 60.0));
        let followed = follow_playhead(viewport(0.0, 50.0), 100.0, 200.0, 0.0);
        assert!((followed.view_start - (100.0 - 0.85 * 50.0)).abs() < 1e-9);
        let early = follow_playhead(viewport(0.0, 50.0), 45.0, 200.0, 0.0);
        assert!((early.view_start - (45.0 - 0.85 * 50.0)).abs() < 1e-9);
        assert!((early.view_end - early.view_start - 50.0).abs() < 1e-9);
    }

    #[test]
    fn builds_and_matches_fit_targets() {
        let targets = build_fit_targets(100.0, 0.0, range(10.0, 30.0), range(10.0, 30.0), true, 45.0, 73.0);
        assert_eq!(targets.iter().map(|target| target.id).collect::<Vec<_>>(), [FitTargetId::All, FitTargetId::Video, FitTargetId::Activity]);
        assert_eq!(matching_fit_target(targets[1].viewport, &targets), Some(FitTargetId::Video));
        assert!(ranges_match(viewport(0.0, 100.0), viewport(0.0, 100.0005)));
    }

    #[test]
    fn the_video_target_fits_one_clip() {
        let videos = [simple("a", 20.0, 0), simple("b", 10.0, 1), simple("c", 20.0, 2)];
        let clips = crate::lib::video_program::collection::join_video_edits(&videos, &edits_at(&videos, &[("b", 15.0), ("c", 60.0)]));
        let id = |clip: Option<&Clip>| clip.map(|clip| clip.id().to_string());
        assert_eq!(id(resolve_fit_video_clip(&clips, Some("c"), 5.0)).as_deref(), Some("c"));
        assert_eq!(id(resolve_fit_video_clip(&clips, None, 17.0)).as_deref(), Some("a"));
        assert_eq!(id(resolve_fit_video_clip(&clips, None, 50.0)).as_deref(), Some("c"));
        assert_eq!(resolve_fit_video_clip(&[], None, 50.0), None);
        let targets = build_fit_targets(80.0, 0.0, range(0.0, 80.0), range(60.0, 80.0), false, 0.0, 73.0);
        assert_eq!(targets.iter().map(|target| target.id).collect::<Vec<_>>(), [FitTargetId::All, FitTargetId::Video]);
        assert_eq!(targets[1].viewport, fit_range_to_viewport(60.0, 80.0, 80.0, 0.0, 0.0));
    }

    #[test]
    fn the_tick_strip_starts_at_its_anchor() {
        let step = timeline_tick_step(9.0);
        let (major, minor) = compute_tick_strip(20.0, step, 9.0, 900.0);
        assert!(step * 9.0 >= 90.0);
        assert_eq!((major[0].second, major[0].x), (20.0, 0.0));
        assert_eq!(major[1].x, step * 9.0);
        assert!(major.last().unwrap().x >= 900.0 + step * 9.0);
        assert!(major[0].label.is_some());
        assert_eq!(minor.len(), (major.len() - 1) * 4);
        assert_eq!(minor[0].label, None);
        assert_eq!(format_tick_label(0.5, 0.2), "0.5s");
        assert_eq!(format_tick_label(65.0, 5.0), "01:05");
    }

    #[test]
    fn pixels_snap_to_the_screen_grid() {
        let metrics = TimelineMetrics { width_px: 500.0, x: 10.25, scale_factor: 2.0, rem: 16.0 };
        assert_eq!(metrics.round_to_device_pixel(10.26), 10.5);
        assert_eq!(metrics.snap_to_screen_pixel(3.0), 3.25, "lands on a screen pixel edge given the timeline's own offset");
    }

    use crate::features::player::reconcile_player;
    use crate::store::slices::media::ActivitySummary;
    use crate::store::slices::video_import::set_video_offset;
    use crate::store::slices::video_import::tests::store_with;
    use ovrley_core::activity::schema::ParsedActivity;

    fn summary(duration_seconds: f64) -> Option<ActivitySummary> {
        Some(ActivitySummary { duration_seconds, ..ActivitySummary::of(&ParsedActivity::default()) })
    }

    fn measured(mut store: Store) -> Store {
        store.player.viewport.metrics.width_px = 500.0;
        reconcile_player(&mut store);
        store
    }

    #[test]
    fn new_media_fits_the_full_range_unless_a_project_restored_the_viewport() {
        let mut store = measured(Store::default());
        assert_eq!(store.editor.timeline_viewport, viewport(0.0, 73.0));
        zoom(&mut store, true);
        reconcile_player(&mut store);
        assert_ne!(store.editor.timeline_viewport, viewport(0.0, 73.0), "the same media keeps the zoom");
        store.media.activity_summary = summary(120.0);
        reconcile_player(&mut store);
        assert_eq!(store.editor.timeline_viewport, viewport(0.0, 120.0));

        store.editor.set_timeline_viewport(viewport(10.0, 20.0)).unwrap();
        store.editor.skip_next_timeline_viewport_reset = true;
        store.media.activity_summary = summary(90.0);
        reconcile_player(&mut store);
        assert_eq!(store.editor.timeline_viewport, viewport(10.0, 20.0));
        assert!(!store.editor.skip_next_timeline_viewport_reset);
    }

    #[test]
    fn an_offset_change_keeps_the_zoom() {
        let mut store = measured(store_with(vec![simple("ride", 30.0, 0)], &[]));
        store.media.activity_summary = summary(100.0);
        reconcile_player(&mut store);
        store.editor.set_timeline_viewport(viewport(10.0, 40.0)).unwrap();
        set_video_offset(&mut store, "ride", -5.0, false).unwrap();
        reconcile_player(&mut store);
        assert_eq!(store.editor.timeline_viewport, viewport(10.0, 40.0));
    }

    #[test]
    fn the_wheel_pans_and_ctrl_wheel_zooms_at_the_pointer() {
        let mut store = measured(Store::default());
        store.media.activity_summary = summary(100.0);
        reconcile_player(&mut store);
        store.editor.set_timeline_viewport(viewport(0.0, 50.0)).unwrap();
        assert!(wheel(&mut store, 100.0, 0.0, false));
        assert_eq!(store.editor.timeline_viewport, viewport(10.0, 60.0), "100 px of a 500 px timeline showing 50 s");
        assert!(wheel(&mut store, -100.0, 250.0, true));
        let view = store.editor.timeline_viewport;
        assert!((view.view_end - view.view_start - 50.0 / 1.6).abs() < 1e-9);
        assert!((view.view_start + (view.view_end - view.view_start) / 2.0 - 35.0).abs() < 1e-9, "the pointer's second stays put");
    }

    #[test]
    fn playback_follows_the_playhead_in_a_zoomed_view() {
        let mut store = measured(Store::default());
        store.media.activity_summary = summary(200.0);
        reconcile_player(&mut store);
        store.editor.set_timeline_viewport(viewport(0.0, 50.0)).unwrap();
        crate::features::player::playback::play(&mut store, std::time::Instant::now());
        reconcile_player(&mut store);
        store.editor.set_selected_second(100.0);
        reconcile_player(&mut store);
        assert!((store.editor.timeline_viewport.view_start - (100.0 - 0.85 * 50.0)).abs() < 1e-9);
    }
}
