//! The activity's events for manual sync: the detector input (`activitySyncInput.js`), stops
//! (`detectStops.js`), turns (`detectTurns.js`) and the combined detection
//! (`detectActivityEvents.js`). Pure: ported line by line, so its numbers are React's.

use ovrley_core::activity::schema::ParsedActivity;
use std::ops::Range;

const SIGNIFICANT_GAP_BASE_SECONDS: f64 = 3.0;
const SIGNIFICANT_GAP_CADENCE_MULTIPLIER: f64 = 3.0;
const KMH_TO_METERS_PER_SECOND: f64 = 1000.0 / 3600.0;
const STOP_ENTRY_DWELL_SECONDS: f64 = 2.0;
const STOP_EXIT_DWELL_SECONDS: f64 = 2.0;
const STOP_EXIT_HYSTERESIS_KMH: f64 = 2.0;
const STOP_MAXIMUM_DURATION_SECONDS: f64 = 30.0;
const TURN_MAXIMUM_DURATION_SECONDS: f64 = 15.0;
const TURN_REVERSAL_TOLERANCE_DEGREES: f64 = 20.0;
/// Rate hysteresis defines episode boundaries independently of the angle threshold.
const TURN_ENTRY_RATE_DEGREES_PER_SECOND: f64 = 10.0;
const TURN_BOUNDARY_RATE_DEGREES_PER_SECOND: f64 = 2.0;
const TURN_EXIT_DWELL_SECONDS: f64 = 1.0;
/// Heading becomes unreliable near standstill; this is independent of stop landmarks.
const TURN_STATIONARY_SPEED_METERS_PER_SECOND: f64 = 0.5;

/// `VIDEO_SYNC_DETECTED_LOCATION_ID`.
pub const DETECTED_LOCATION_ID: &str = "detected-course-location";

/// Which metrics the activity has.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Availability {
    pub speed: bool,
    pub heading: bool,
    pub course: bool,
}

/// The one detector input shape (`createActivitySyncInput`): aligned series with explicit gaps, and
/// the contiguous sample segments.
#[derive(Clone, Debug, PartialEq)]
pub struct SyncInput {
    pub elapsed_seconds: Vec<f64>,
    pub speed: Vec<Option<f64>>,
    pub heading: Vec<Option<f64>>,
    pub availability: Availability,
    pub segments: Vec<Range<usize>>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Interval {
    pub start: f64,
    pub end: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StopEvent {
    pub id: String,
    pub time: f64,
    pub low_speed_interval: Interval,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TurnDirection {
    Left,
    Right,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TurnEvent {
    pub id: String,
    pub direction: TurnDirection,
    pub start: f64,
    pub end: f64,
    pub signed_change: f64,
    pub representative_time: f64,
}

/// The course location the user picked on the map (its id is [`DETECTED_LOCATION_ID`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LocationEvent {
    pub time: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GraphPoint {
    pub time: f64,
    pub value: Option<f64>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct GraphSeries {
    pub speed: Vec<GraphPoint>,
    pub turning: Vec<GraphPoint>,
}

/// The canonical detected-event model.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Detection {
    pub availability: Availability,
    pub stops: Vec<StopEvent>,
    pub turns: Vec<TurnEvent>,
    pub location: Option<LocationEvent>,
    pub graph_series: GraphSeries,
}

impl Detection {
    /// `createEmptyVideoSyncDetection` with the saved location (`detectionWithSavedLocation`).
    pub fn with_location(location_second: Option<f64>) -> Self {
        Self {
            location: location_second.map(|time| LocationEvent { time }),
            ..Self::default()
        }
    }

    pub fn turn_count(&self, direction: TurnDirection) -> usize {
        self.turns
            .iter()
            .filter(|turn| turn.direction == direction)
            .count()
    }
}

/// The detector's physical thresholds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DetectionSettings {
    pub speed_threshold_kmh: f64,
    pub turn_threshold_degrees: f64,
}

fn median(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut ordered = values.to_vec();
    ordered.sort_by(f64::total_cmp);
    let middle = ordered.len() / 2;
    if ordered.len().is_multiple_of(2) {
        (ordered[middle - 1] + ordered[middle]) / 2.0
    } else {
        ordered[middle]
    }
}

/// Splits the activity where elapsed-time gaps exceed the cadence-aware threshold.
fn build_segments(elapsed_seconds: &[f64]) -> Vec<Range<usize>> {
    if elapsed_seconds.is_empty() {
        return Vec::new();
    }
    let intervals: Vec<f64> = elapsed_seconds
        .windows(2)
        .map(|pair| pair[1] - pair[0])
        .collect();
    let significant_gap =
        SIGNIFICANT_GAP_BASE_SECONDS.max(SIGNIFICANT_GAP_CADENCE_MULTIPLIER * median(&intervals));
    let mut segments = Vec::new();
    let mut start = 0;
    for index in 1..elapsed_seconds.len() {
        if elapsed_seconds[index] - elapsed_seconds[index - 1] > significant_gap {
            segments.push(start..index);
            start = index;
        }
    }
    segments.push(start..elapsed_seconds.len());
    segments
}

fn aligned(series: &[Option<f64>], length: usize) -> Vec<Option<f64>> {
    (0..length)
        .map(|index| series.get(index).copied().flatten())
        .collect()
}

/// `createActivitySyncInput`.
pub fn create_sync_input(activity: &ParsedActivity) -> SyncInput {
    let length = activity.sample_elapsed_seconds.len();
    let speed = aligned(&activity.speed, length);
    let heading = aligned(&activity.heading, length);
    let course = (0..length).any(|index| {
        matches!(
            activity.sample_course_points.get(index),
            Some((Some(_), Some(_)))
        )
    });
    SyncInput {
        elapsed_seconds: activity.sample_elapsed_seconds.clone(),
        availability: Availability {
            speed: speed.iter().any(Option::is_some),
            heading: heading.iter().any(Option::is_some),
            course,
        },
        speed,
        heading,
        segments: build_segments(&activity.sample_elapsed_seconds),
    }
}

/// `detectActivityEventsFromInput`: `location` is the user's pick, kept across detections.
pub fn detect_activity_events(
    input: &SyncInput,
    settings: DetectionSettings,
    location: Option<LocationEvent>,
) -> Detection {
    let stops = detect_stops(input, settings.speed_threshold_kmh);
    let turning = derive_turning_series(input);
    let turns = detect_turns_from_series(input, &turning, settings.turn_threshold_degrees);
    Detection {
        availability: input.availability,
        stops,
        turns,
        location,
        graph_series: GraphSeries {
            speed: input
                .elapsed_seconds
                .iter()
                .zip(&input.speed)
                .map(|(&time, &value)| GraphPoint { time, value })
                .collect(),
            turning,
        },
    }
}

// --- Stops -------------------------------------------------------------------------------------

fn interpolate_crossing_time(
    previous_time: f64,
    previous_value: f64,
    time: f64,
    value: f64,
    threshold: f64,
) -> f64 {
    let delta = value - previous_value;
    if delta == 0.0 {
        return previous_time;
    }
    previous_time + (threshold - previous_value) / delta * (time - previous_time)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StopPhase {
    Unestablished,
    EstablishingMovement,
    Moving,
    EnteringStop,
    Stopped,
    ExitingStop,
}

#[derive(Clone, Copy, Debug)]
struct StopState {
    phase: StopPhase,
    movement_started_at: Option<f64>,
    near_stop_started_at: Option<f64>,
    stop_candidate_started_at: Option<f64>,
    exit_started_at: Option<f64>,
    /// The index of the stop event in the output.
    stop_event: Option<usize>,
    stop_interval_open: bool,
}

impl StopState {
    fn new(phase: StopPhase) -> Self {
        Self {
            phase,
            movement_started_at: None,
            near_stop_started_at: None,
            stop_candidate_started_at: None,
            exit_started_at: None,
            stop_event: None,
            stop_interval_open: false,
        }
    }
}

fn extend_stop_interval(state: &mut StopState, end_time: f64, stops: &mut [StopEvent]) {
    let Some(index) = state.stop_event.filter(|_| state.stop_interval_open) else {
        return;
    };
    let interval = &mut stops[index].low_speed_interval;
    interval.end = end_time.min(interval.start + STOP_MAXIMUM_DURATION_SECONDS);
    if interval.end == interval.start + STOP_MAXIMUM_DURATION_SECONDS {
        state.stop_interval_open = false;
    }
}

fn enter_stopped(previous: &StopState, near_stop_started_at: Option<f64>) -> StopState {
    StopState {
        near_stop_started_at,
        stop_event: previous.stop_event,
        stop_interval_open: previous.stop_interval_open,
        ..StopState::new(StopPhase::Stopped)
    }
}

fn moving_since(time: f64) -> StopState {
    StopState {
        movement_started_at: Some(time),
        ..StopState::new(StopPhase::EstablishingMovement)
    }
}

fn state_for_first_sample(
    speed: f64,
    time: f64,
    threshold: f64,
    movement_threshold: f64,
) -> StopState {
    if speed <= threshold {
        return enter_stopped(&StopState::new(StopPhase::Unestablished), Some(time));
    }
    if speed > movement_threshold {
        return moving_since(time);
    }
    StopState::new(StopPhase::Unestablished)
}

struct StopSample {
    previous_time: f64,
    previous_speed: f64,
    time: f64,
    speed: f64,
    threshold: f64,
    movement_threshold: f64,
}

/// A sample may finish one phase and enter the next in the same transition.
fn advance_state(state: StopState, sample: &StopSample, stops: &mut Vec<StopEvent>) -> StopState {
    let StopSample {
        previous_time,
        previous_speed,
        time,
        speed,
        threshold,
        movement_threshold,
    } = *sample;
    let mut next = state;
    loop {
        match next.phase {
            StopPhase::Unestablished => {
                if speed <= threshold {
                    next = enter_stopped(&next, Some(time));
                    continue;
                }
                if speed > movement_threshold {
                    return moving_since(time);
                }
                return next;
            }
            StopPhase::EstablishingMovement => {
                let movement_end = if speed > movement_threshold {
                    time
                } else if previous_speed > movement_threshold {
                    interpolate_crossing_time(
                        previous_time,
                        previous_speed,
                        time,
                        speed,
                        movement_threshold,
                    )
                } else {
                    previous_time
                };
                let started = next.movement_started_at.expect("movement has a start");
                if movement_end - started < STOP_ENTRY_DWELL_SECONDS {
                    return if speed > movement_threshold {
                        next
                    } else {
                        StopState::new(StopPhase::Unestablished)
                    };
                }
                next = StopState {
                    movement_started_at: Some(started),
                    ..StopState::new(StopPhase::Moving)
                };
            }
            StopPhase::Moving => {
                if previous_speed > threshold && speed <= threshold {
                    let crossing = interpolate_crossing_time(
                        previous_time,
                        previous_speed,
                        time,
                        speed,
                        threshold,
                    );
                    next = StopState {
                        near_stop_started_at: Some(crossing),
                        stop_candidate_started_at: Some(crossing),
                        ..StopState::new(StopPhase::EnteringStop)
                    };
                    continue;
                }
                return next;
            }
            StopPhase::EnteringStop => {
                let candidate = next
                    .stop_candidate_started_at
                    .expect("a stop candidate has a start");
                if speed <= threshold && time - candidate >= STOP_ENTRY_DWELL_SECONDS {
                    let near_stop = next
                        .near_stop_started_at
                        .expect("a stop candidate is near a stop");
                    stops.push(StopEvent {
                        id: format!("stop-{}", stops.len()),
                        time: candidate,
                        low_speed_interval: Interval {
                            start: near_stop,
                            end: time,
                        },
                    });
                    return StopState {
                        near_stop_started_at: Some(near_stop),
                        stop_event: Some(stops.len() - 1),
                        stop_interval_open: true,
                        ..StopState::new(StopPhase::Stopped)
                    };
                }
                if speed > threshold {
                    next = enter_stopped(&next, next.near_stop_started_at);
                    continue;
                }
                return next;
            }
            StopPhase::Stopped => {
                if next.stop_interval_open && previous_speed <= threshold && speed > threshold {
                    extend_stop_interval(
                        &mut next,
                        interpolate_crossing_time(
                            previous_time,
                            previous_speed,
                            time,
                            speed,
                            threshold,
                        ),
                        stops,
                    );
                    next.stop_interval_open = false;
                } else if speed <= threshold {
                    extend_stop_interval(&mut next, time, stops);
                }
                if speed > movement_threshold {
                    let exit_started_at = if previous_speed <= movement_threshold {
                        interpolate_crossing_time(
                            previous_time,
                            previous_speed,
                            time,
                            speed,
                            movement_threshold,
                        )
                    } else {
                        previous_time
                    };
                    return StopState {
                        near_stop_started_at: next.near_stop_started_at,
                        exit_started_at: Some(exit_started_at),
                        stop_event: next.stop_event,
                        stop_interval_open: next.stop_interval_open,
                        ..StopState::new(StopPhase::ExitingStop)
                    };
                }
                return next;
            }
            StopPhase::ExitingStop => {
                let exit_time = next.exit_started_at.expect("an exit has a start");
                if speed > movement_threshold && time - exit_time >= STOP_EXIT_DWELL_SECONDS {
                    extend_stop_interval(&mut next, exit_time, stops);
                    return StopState {
                        movement_started_at: Some(exit_time),
                        ..StopState::new(StopPhase::Moving)
                    };
                }
                if speed <= movement_threshold {
                    return StopState {
                        near_stop_started_at: next.near_stop_started_at,
                        stop_event: next.stop_event,
                        stop_interval_open: next.stop_interval_open,
                        ..StopState::new(StopPhase::Stopped)
                    };
                }
                return next;
            }
        }
    }
}

/// `detectStops`: stop transitions from established movement.
pub fn detect_stops(input: &SyncInput, speed_threshold_kmh: f64) -> Vec<StopEvent> {
    let threshold = speed_threshold_kmh * KMH_TO_METERS_PER_SECOND;
    let movement_threshold =
        (speed_threshold_kmh + STOP_EXIT_HYSTERESIS_KMH) * KMH_TO_METERS_PER_SECOND;
    let mut stops = Vec::new();
    for segment in &input.segments {
        let mut state = StopState::new(StopPhase::Unestablished);
        let mut previous: Option<usize> = None;
        for index in segment.clone() {
            let Some(speed) = input.speed[index] else {
                if let Some(previous) = previous {
                    extend_stop_interval(&mut state, input.elapsed_seconds[previous], &mut stops);
                }
                state = StopState::new(StopPhase::Unestablished);
                previous = None;
                continue;
            };
            let time = input.elapsed_seconds[index];
            match previous {
                None => state = state_for_first_sample(speed, time, threshold, movement_threshold),
                Some(previous_index) => {
                    let sample = StopSample {
                        previous_time: input.elapsed_seconds[previous_index],
                        previous_speed: input.speed[previous_index]
                            .expect("the previous sample has a speed"),
                        time,
                        speed,
                        threshold,
                        movement_threshold,
                    };
                    state = advance_state(state, &sample, &mut stops);
                }
            }
            previous = Some(index);
        }
        if let Some(previous) = previous {
            extend_stop_interval(&mut state, input.elapsed_seconds[previous], &mut stops);
        }
    }
    stops
}

// --- Turns -------------------------------------------------------------------------------------

/// The shortest signed heading change in (-180, 180] degrees.
fn signed_heading_delta(current: f64, previous: f64) -> f64 {
    let delta = ((current - previous + 180.0) % 360.0 + 360.0) % 360.0;
    if delta == 0.0 {
        180.0
    } else {
        delta - 180.0
    }
}

/// `deriveTurningSeries`: interval-average turn rates from the finalized heading. Each value
/// describes the interval ending at its timestamp; missing headings and segments break the series.
pub fn derive_turning_series(input: &SyncInput) -> Vec<GraphPoint> {
    let mut series: Vec<GraphPoint> = input
        .elapsed_seconds
        .iter()
        .map(|&time| GraphPoint { time, value: None })
        .collect();
    for segment in &input.segments {
        for (index, point) in series.iter_mut().enumerate().take(segment.end).skip(segment.start + 1) {
            let (Some(previous), Some(current)) = (input.heading[index - 1], input.heading[index])
            else {
                continue;
            };
            let elapsed = input.elapsed_seconds[index] - input.elapsed_seconds[index - 1];
            point.value = Some(signed_heading_delta(current, previous) / elapsed);
        }
    }
    series
}

/// Suppresses heading only near standstill. Speed crossings are interpolated; missing speed cannot
/// establish standstill, and neither missing samples nor timestamp gaps are bridged.
fn derive_near_stop_intervals(input: &SyncInput) -> Vec<Interval> {
    let mut intervals: Vec<Interval> = Vec::new();
    let threshold = TURN_STATIONARY_SPEED_METERS_PER_SECOND;
    for segment in &input.segments {
        for index in segment.start + 1..segment.end {
            let (Some(previous_speed), Some(speed)) = (input.speed[index - 1], input.speed[index])
            else {
                continue;
            };
            if previous_speed > threshold && speed > threshold {
                continue;
            }
            let previous_time = input.elapsed_seconds[index - 1];
            let time = input.elapsed_seconds[index];
            let (mut start, mut end) = (previous_time, time);
            if previous_speed > threshold || speed > threshold {
                let crossing = previous_time
                    + (threshold - previous_speed) / (speed - previous_speed)
                        * (time - previous_time);
                if previous_speed > threshold {
                    start = crossing;
                } else {
                    end = crossing;
                }
            }
            if start == end {
                continue;
            }
            match intervals.last_mut() {
                Some(last) if last.end == start => last.end = end,
                _ => intervals.push(Interval { start, end }),
            }
        }
    }
    intervals
}

/// A piece of a sample interval: its interval-average rate, or `None` near standstill.
#[derive(Clone, Copy, Debug)]
struct RateInterval {
    start: f64,
    end: f64,
    rate: Option<f64>,
}

/// An active or quiet piece inside an episode.
#[derive(Clone, Copy, Debug)]
struct EpisodeInterval {
    start: f64,
    end: f64,
    rate: f64,
}

/// Splits sample intervals at the sorted suppression boundaries (`createIntervalReader`).
struct IntervalReader<'a> {
    near_stop: &'a [Interval],
    index: usize,
}

impl IntervalReader<'_> {
    fn read(
        &mut self,
        mut start: f64,
        end: f64,
        rate: Option<f64>,
        mut emit: impl FnMut(RateInterval),
    ) {
        let near_stop = self.near_stop;
        while self.index < near_stop.len() && near_stop[self.index].end <= start {
            self.index += 1;
        }
        while self.index < near_stop.len() && near_stop[self.index].start < end {
            let suppressed = near_stop[self.index];
            if start < suppressed.start {
                emit(RateInterval {
                    start,
                    end: suppressed.start,
                    rate,
                });
            }
            let suppressed_end = end.min(suppressed.end);
            emit(RateInterval {
                start: start.max(suppressed.start),
                end: suppressed_end,
                rate: None,
            });
            start = suppressed_end;
            if suppressed.end > end {
                break;
            }
            self.index += 1;
        }
        if start < end {
            emit(RateInterval { start, end, rate });
        }
    }
}

/// Qualifies one directional episode (`emitTurn`).
fn emit_turn(
    intervals: &[EpisodeInterval],
    mut first: usize,
    mut end: usize,
    direction: f64,
    threshold: f64,
    events: &mut Vec<TurnEvent>,
) {
    while first < end && intervals[first].rate.abs() < TURN_BOUNDARY_RATE_DEGREES_PER_SECOND {
        first += 1;
    }
    while end > first && intervals[end - 1].rate.abs() < TURN_BOUNDARY_RATE_DEGREES_PER_SECOND {
        end -= 1;
    }
    if first == end {
        return;
    }
    let mut signed_change = 0.0;
    let mut peak_rate: f64 = 0.0;
    for interval in &intervals[first..end] {
        signed_change += interval.rate * (interval.end - interval.start);
        peak_rate = peak_rate.max(direction * interval.rate);
    }
    let start = intervals[first].start;
    let end_time = intervals[end - 1].end;
    if peak_rate < TURN_ENTRY_RATE_DEGREES_PER_SECOND
        || direction * signed_change < threshold
        || end_time - start > TURN_MAXIMUM_DURATION_SECONDS
    {
        return;
    }
    let (direction, name) = if direction > 0.0 {
        (TurnDirection::Right, "rightTurn")
    } else {
        (TurnDirection::Left, "leftTurn")
    };
    events.push(TurnEvent {
        id: format!("{name}-{}", events.len()),
        direction,
        start,
        end: end_time,
        signed_change,
        representative_time: (start + end_time) / 2.0,
    });
}

/// Splits an episode at confirmed heading reversals (`finishEpisode`).
fn finish_episode(intervals: &[EpisodeInterval], threshold: f64, events: &mut Vec<TurnEvent>) {
    let Some(first_interval) = intervals.first() else {
        return;
    };
    let mut first = 0;
    let mut direction = first_interval.rate.signum();
    let mut heading_change = 0.0;
    let mut extreme_change = 0.0;
    let mut extreme_end = 0;
    for (index, interval) in intervals.iter().enumerate() {
        heading_change += interval.rate * (interval.end - interval.start);
        if direction * (heading_change - extreme_change) >= 0.0 {
            extreme_change = heading_change;
            extreme_end = index + 1;
        } else if direction * (extreme_change - heading_change) > TURN_REVERSAL_TOLERANCE_DEGREES {
            emit_turn(intervals, first, extreme_end, direction, threshold, events);
            first = extreme_end;
            direction = -direction;
            extreme_change = heading_change;
            extreme_end = index + 1;
        }
    }
    emit_turn(
        intervals,
        first,
        intervals.len(),
        direction,
        threshold,
        events,
    );
}

/// Collects episodes with an elapsed-time exit dwell (`createEpisodeCollector`).
struct EpisodeCollector {
    threshold: f64,
    intervals: Vec<EpisodeInterval>,
    quiet: Vec<EpisodeInterval>,
    events: Vec<TurnEvent>,
}

impl EpisodeCollector {
    fn finish(&mut self) {
        finish_episode(&self.intervals, self.threshold, &mut self.events);
        self.intervals.clear();
        self.quiet.clear();
    }

    fn add(&mut self, interval: RateInterval) {
        let Some(rate) = interval.rate else {
            self.finish();
            return;
        };
        let interval = EpisodeInterval {
            start: interval.start,
            end: interval.end,
            rate,
        };
        if rate.abs() < TURN_BOUNDARY_RATE_DEGREES_PER_SECOND {
            if self.intervals.is_empty() {
                return;
            }
            self.quiet.push(interval);
            if interval.end - self.quiet[0].start >= TURN_EXIT_DWELL_SECONDS {
                self.finish();
            }
            return;
        }
        self.intervals.append(&mut self.quiet);
        self.intervals.push(interval);
    }
}

/// `detectTurnsFromDerivedSeries`: turns from the same rates the graph shows.
pub fn detect_turns_from_series(
    input: &SyncInput,
    turning: &[GraphPoint],
    turn_threshold_degrees: f64,
) -> Vec<TurnEvent> {
    let near_stop = derive_near_stop_intervals(input);
    let mut reader = IntervalReader {
        near_stop: &near_stop,
        index: 0,
    };
    let mut collector = EpisodeCollector {
        threshold: turn_threshold_degrees,
        intervals: Vec::new(),
        quiet: Vec::new(),
        events: Vec::new(),
    };
    for segment in &input.segments {
        for (index, point) in turning.iter().enumerate().take(segment.end).skip(segment.start + 1) {
            reader.read(
                input.elapsed_seconds[index - 1],
                input.elapsed_seconds[index],
                point.value,
                |interval| collector.add(interval),
            );
        }
        collector.finish();
    }
    collector.events
}

#[cfg(test)]
mod tests {
    use super::*;

    const SETTINGS: DetectionSettings = DetectionSettings {
        speed_threshold_kmh: 5.0,
        turn_threshold_degrees: 90.0,
    };

    fn make_activity(
        sample_rate: f64,
        duration_seconds: f64,
        speed_at: impl Fn(f64) -> f64,
        heading_at: impl Fn(f64) -> f64,
        course: bool,
    ) -> ParsedActivity {
        let count = (duration_seconds * sample_rate).round() as usize;
        let elapsed: Vec<f64> = (0..=count)
            .map(|index| index as f64 / sample_rate)
            .collect();
        ParsedActivity {
            speed: elapsed.iter().map(|&time| Some(speed_at(time))).collect(),
            heading: elapsed.iter().map(|&time| Some(heading_at(time))).collect(),
            sample_course_points: if course {
                elapsed.iter().map(|_| (Some(50.0), Some(14.0))).collect()
            } else {
                Vec::new()
            },
            sample_elapsed_seconds: elapsed,
            ..ParsedActivity::default()
        }
    }

    fn detect(activity: &ParsedActivity) -> Detection {
        detect_activity_events(&create_sync_input(activity), SETTINGS, None)
    }

    fn stops_of(activity: &ParsedActivity) -> Vec<StopEvent> {
        detect_stops(&create_sync_input(activity), SETTINGS.speed_threshold_kmh)
    }

    fn close(left: f64, right: f64, digits: i32) -> bool {
        (left - right).abs() < 10f64.powi(-digits) / 2.0
    }

    #[test]
    fn detects_equivalent_stop_and_turn_times_at_1_and_40_hz() {
        let combined = |rate: f64| {
            make_activity(
                rate,
                16.0,
                |time| {
                    if time < 4.0 {
                        3.0
                    } else if time < 5.0 {
                        3.0 - 2.5 * (time - 4.0)
                    } else if time < 8.0 {
                        0.5
                    } else {
                        3.0
                    }
                },
                |time| (time * 30.0).min(120.0),
                true,
            )
        };
        let one = detect(&combined(1.0));
        let forty = detect(&combined(40.0));
        assert_eq!(
            (
                one.stops.len(),
                forty.stops.len(),
                one.turns.len(),
                forty.turns.len()
            ),
            (1, 1, 1, 1)
        );
        assert!(close(forty.stops[0].time, one.stops[0].time, 1));
        assert!(close(
            forty.turns[0].representative_time,
            one.turns[0].representative_time,
            0
        ));
        assert_eq!(one.turns[0].direction, TurnDirection::Right);
    }

    #[test]
    fn uses_dwell_and_hysteresis_for_one_stop_and_ignores_a_stationary_start() {
        let drift = stops_of(&make_activity(
            1.0,
            18.0,
            |time| {
                if time < 4.0 {
                    0.0
                } else if time < 7.0 {
                    0.5
                } else if time < 11.0 {
                    3.0
                } else {
                    0.5
                }
            },
            |_| 0.0,
            true,
        ));
        let stationary = stops_of(&make_activity(
            1.0,
            12.0,
            |time| if time < 4.0 { 0.5 } else { 3.0 },
            |_| 0.0,
            true,
        ));
        assert_eq!(drift.len(), 1);
        assert!(drift[0].time > 3.0);
        assert!(drift[0].low_speed_interval.end - drift[0].low_speed_interval.start >= 2.0);
        assert!(stationary.is_empty());
    }

    #[test]
    fn ends_a_stop_interval_when_speed_crosses_above_the_threshold() {
        let stops = stops_of(&make_activity(
            1.0,
            16.0,
            |time| {
                if time < 4.0 {
                    3.0
                } else if time < 10.0 {
                    0.5
                } else {
                    1.6
                }
            },
            |_| 0.0,
            true,
        ));
        assert_eq!(stops.len(), 1);
        assert!(stops[0].low_speed_interval.end > 9.0 && stops[0].low_speed_interval.end < 10.0);
    }

    #[test]
    fn caps_a_stop_interval_at_thirty_seconds() {
        let stops = stops_of(&make_activity(
            1.0,
            50.0,
            |time| if time < 4.0 { 3.0 } else { 0.5 },
            |_| 0.0,
            true,
        ));
        assert_eq!(stops.len(), 1);
        assert!(close(
            stops[0].low_speed_interval.end - stops[0].low_speed_interval.start,
            30.0,
            2
        ));
    }

    #[test]
    fn classifies_wraparound_and_meaningful_reversal() {
        let wraparound = detect(&make_activity(
            1.0,
            6.0,
            |_| 3.0,
            |time| (350.0 + time * 35.0) % 360.0,
            true,
        ));
        let reversal = detect(&make_activity(
            1.0,
            12.0,
            |_| 3.0,
            |time| {
                if time <= 4.0 {
                    time * 30.0
                } else {
                    120.0 - (time - 4.0) * 30.0
                }
            },
            true,
        ));
        assert!(wraparound
            .turns
            .iter()
            .any(|turn| turn.direction == TurnDirection::Right));
        assert_eq!(
            reversal
                .turns
                .iter()
                .map(|turn| turn.direction)
                .collect::<Vec<_>>(),
            [TurnDirection::Right, TurnDirection::Left]
        );
    }

    #[test]
    fn keeps_a_turn_through_a_minor_opposite_correction() {
        let input = SyncInput {
            elapsed_seconds: vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0],
            speed: vec![Some(3.0); 6],
            heading: vec![None; 6],
            availability: Availability::default(),
            segments: vec![Range { start: 0, end: 6 }],
        };
        let turning: Vec<GraphPoint> = [
            None,
            Some(40.0),
            Some(40.0),
            Some(-15.0),
            Some(25.0),
            Some(0.0),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, value)| GraphPoint {
            time: index as f64,
            value,
        })
        .collect();
        let turns = detect_turns_from_series(&input, &turning, SETTINGS.turn_threshold_degrees);
        assert_eq!(turns.len(), 1);
        assert_eq!(turns[0].direction, TurnDirection::Right);
    }

    #[test]
    fn rejects_a_gradual_bend_longer_than_the_qualifying_duration() {
        assert!(
            detect(&make_activity(1.0, 25.0, |_| 3.0, |time| time * 8.0, true))
                .turns
                .is_empty()
        );
    }

    #[test]
    fn finds_a_delayed_turn_and_ends_it_with_the_heading_change() {
        let delayed = |rate: f64| {
            make_activity(
                rate,
                20.0,
                |_| 3.0,
                |time| {
                    if time < 8.0 {
                        time
                    } else if time < 11.0 {
                        8.0 + (time - 8.0) * 40.0
                    } else {
                        128.0
                    }
                },
                true,
            )
        };
        let one = detect(&delayed(1.0));
        let forty = detect(&delayed(40.0));
        assert_eq!((one.turns.len(), forty.turns.len()), (1, 1));
        assert_eq!(one.turns[0].direction, TurnDirection::Right);
        assert!(one.turns[0].end < 14.0 && forty.turns[0].end < 14.0);
    }

    #[test]
    fn does_not_bridge_a_significant_gap_and_reports_missing_channels() {
        let mut activity = make_activity(1.0, 8.0, |_| 3.0, |_| 0.0, false);
        activity.sample_elapsed_seconds = vec![0.0, 1.0, 2.0, 3.0, 4.0, 14.0, 15.0, 16.0];
        activity.speed = [3.0, 3.0, 3.0, 3.0, 3.0, 0.5, 0.5, 0.5].map(Some).to_vec();
        activity.heading = vec![None; 8];
        let input = create_sync_input(&activity);
        let result = detect(&activity);
        assert_eq!(input.segments, vec![0..5, 5..8]);
        assert!(result.stops.is_empty() && result.turns.is_empty());
        assert_eq!(
            result.availability,
            Availability {
                speed: true,
                heading: false,
                course: false
            }
        );
        assert!(result
            .graph_series
            .turning
            .iter()
            .all(|point| point.value.is_none()));
    }

    #[test]
    fn returns_the_same_turning_series_for_direct_and_combined_detection() {
        let activity = make_activity(10.0, 6.0, |_| 3.0, |time| time * 30.0, true);
        let input = create_sync_input(&activity);
        let direct = derive_turning_series(&input);
        let combined = detect(&activity);
        assert_eq!(combined.graph_series.turning, direct);
        assert_eq!(
            combined.turns,
            detect_turns_from_series(&input, &direct, SETTINGS.turn_threshold_degrees)
        );
    }
}
