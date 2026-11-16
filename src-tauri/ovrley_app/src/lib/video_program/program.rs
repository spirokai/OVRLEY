//! Port of `program.js` and `playback-target.js`: the output program (ordered pieces and the joins
//! between them) of the collection, what it shows at a timeline second, and where playback goes.
//!
//! A piece is a continuous source range of one clip: a segment that is not excluded. Overlapping
//! pieces are resolved for playback: the earlier-starting clip plays to its end and the later clip
//! continues at the matching source position. Overlaps are still reported, so single-file export
//! can be blocked. Excluded segments are listed too: they stay on the timeline and in the preview.
//!
//! A join connects consecutive pieces and belongs to the incoming piece's segment, whose
//! `transition_in` is the join's transition. `None` follows the project defaults, snapped to the
//! frames of the joined pieces. Where the default does not fit, the join is a hard cut and its
//! `skipped_default` says which piece it did not fit and how much room that piece has.

use super::collection::sort_clips_by_timeline;
use super::segments::{list_video_segments, segment_join_key};
use super::{Clip, Transition, TransitionKind};
use ovrley_core::encode::ffmpeg::transitions::TransitionDefaults;
use ovrley_core::encode::fps::Fps;
use std::collections::HashMap;

const TIME_EPSILON_SECONDS: f64 = 1e-6;

#[derive(Clone, Debug, PartialEq)]
pub struct Piece {
    pub id: String,
    pub video_id: String,
    /// A piece of a missing video keeps its place; the preview shows that the file was not found.
    pub missing: bool,
    pub import_order: u32,
    pub fps: Fps,
    pub source_start_frame: i64,
    pub source_end_frame: i64,
    pub source_start_second: f64,
    pub source_end_second: f64,
    pub timeline_start_second: f64,
    pub timeline_end_second: f64,
    pub duration_seconds: f64,
    pub segment_key: String,
    pub skip_overlay: bool,
}

/// The piece a default transition did not fit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JoinSide {
    Previous,
    Next,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkippedDefault {
    pub side: JoinSide,
    pub duration_seconds: f64,
    pub room_seconds: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Join {
    pub key: String,
    pub video_id: String,
    pub segment_key: String,
    pub from_piece_id: String,
    pub to_piece_id: String,
    pub gap_seconds: f64,
    pub transition: Transition,
    pub is_default: bool,
    pub skipped_default: Option<SkippedDefault>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Overlap {
    pub earlier_video_id: String,
    pub later_video_id: String,
    pub start_second: f64,
    pub end_second: f64,
}

/// The program in playback order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Program {
    pub pieces: Vec<Piece>,
    pub joins: Vec<Join>,
    pub overlaps: Vec<Overlap>,
    pub excluded_segments: Vec<Piece>,
    pub duration_seconds: f64,
}

impl Program {
    pub fn piece(&self, id: &str) -> &Piece {
        self.pieces.iter().find(|piece| piece.id == id).expect("joins reference pieces of their program")
    }
}

fn create_piece(clip: &Clip, start_frame: i64, end_frame: i64, segment_key: &str, skip_overlay: bool) -> Piece {
    let fps = clip.fps();
    let source_start_second = fps.frames_to_seconds(start_frame);
    let source_end_second = fps.frames_to_seconds(end_frame);
    Piece {
        id: format!("{}:{start_frame}", clip.id()),
        video_id: clip.id().to_string(),
        missing: clip.video.missing,
        import_order: clip.video.import_order,
        fps,
        source_start_frame: start_frame,
        source_end_frame: end_frame,
        source_start_second,
        source_end_second,
        timeline_start_second: clip.offset_seconds() + source_start_second,
        timeline_end_second: clip.offset_seconds() + source_end_second,
        duration_seconds: source_end_second - source_start_second,
        segment_key: segment_key.to_string(),
        skip_overlay,
    }
}

fn collect_pieces(clips: &[&Clip]) -> Vec<Piece> {
    let mut pieces: Vec<Piece> = clips
        .iter()
        .flat_map(|clip| {
            list_video_segments(clip)
                .into_iter()
                .filter(|segment| !segment.edits.excluded && segment.end_frame > segment.start_frame)
                .map(move |segment| create_piece(clip, segment.start_frame, segment.end_frame, &segment.key, segment.edits.skip_overlay))
        })
        .collect();
    pieces.sort_by(|left, right| {
        left.timeline_start_second
            .partial_cmp(&right.timeline_start_second)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(left.import_order.cmp(&right.import_order))
            .then(left.source_start_frame.cmp(&right.source_start_frame))
    });
    pieces
}

fn collect_excluded_segments(clips: &[Clip]) -> Vec<Piece> {
    clips
        .iter()
        .flat_map(|clip| {
            list_video_segments(clip)
                .into_iter()
                .filter(|segment| segment.edits.excluded)
                .map(move |segment| create_piece(clip, segment.start_frame, segment.end_frame, &segment.key, segment.edits.skip_overlay))
        })
        .collect()
}

fn resolve_overlaps(sorted: Vec<Piece>, clips_by_id: &HashMap<&str, &Clip>) -> (Vec<Piece>, Vec<Overlap>) {
    let mut pieces = Vec::new();
    let mut overlaps = Vec::new();
    let mut running_end = f64::NEG_INFINITY;
    let mut running_video: Option<String> = None;
    for piece in sorted {
        let overlap_seconds = running_end - piece.timeline_start_second;
        // Less than half a frame is no overlap: not one frame is shown twice.
        if overlap_seconds < piece.fps.frames_to_seconds(1) / 2.0 {
            running_end = piece.timeline_end_second;
            running_video = Some(piece.video_id.clone());
            pieces.push(piece);
            continue;
        }
        overlaps.push(Overlap {
            earlier_video_id: running_video.clone().expect("an overlap follows a piece"),
            later_video_id: piece.video_id.clone(),
            start_second: piece.timeline_start_second,
            end_second: running_end.min(piece.timeline_end_second),
        });
        let (num, den) = piece.fps.components();
        let trimmed_start = piece.source_start_frame + (overlap_seconds * num as f64 / den as f64 - TIME_EPSILON_SECONDS).ceil() as i64;
        if trimmed_start < piece.source_end_frame {
            pieces.push(create_piece(clips_by_id[piece.video_id.as_str()], trimmed_start, piece.source_end_frame, &piece.segment_key, piece.skip_overlay));
        }
        if piece.timeline_end_second > running_end {
            running_end = piece.timeline_end_second;
            running_video = Some(piece.video_id);
        }
    }
    (pieces, overlaps)
}

fn transition_override(piece: &Piece, clips_by_id: &HashMap<&str, &Clip>) -> Option<Transition> {
    clips_by_id[piece.video_id.as_str()].segment(&piece.segment_key).transition_in
}

/// Seconds a piece leaves for a transition at one end, keeping one untouched frame beside the
/// transition at its other end.
fn room_seconds(piece: &Piece, other_end_seconds: f64) -> f64 {
    piece.duration_seconds - other_end_seconds - piece.fps.frames_to_seconds(1)
}

/// `resolveJoinTransition`: a stored override wins. Separate clips closer than the continuity
/// threshold (a camera's split recording) are joined with a hard cut; other joins get the default
/// where it fits both pieces beside the transitions they already have.
fn resolve_join_transition(
    override_: Option<Transition>,
    previous: &Piece,
    next: &Piece,
    gap_seconds: f64,
    previous_incoming_seconds: f64,
    next_outgoing_seconds: f64,
    defaults: &TransitionDefaults,
) -> (Transition, Option<SkippedDefault>) {
    if let Some(transition) = override_ {
        return (transition, None);
    }
    let is_camera_split = previous.video_id != next.video_id && gap_seconds <= defaults.split_gap_threshold_seconds + TIME_EPSILON_SECONDS;
    let kind = TransitionKind::from_name(&defaults.transition_type).expect("the catalogue's defaults name a catalogue transition");
    let TransitionKind::Xfade(name) = kind else { return (Transition::HARD_CUT, None) };
    if is_camera_split {
        return (Transition::HARD_CUT, None);
    }
    let duration_seconds = previous.fps.snap_duration_to_frames(defaults.duration_seconds);
    let sides = [
        (JoinSide::Previous, room_seconds(previous, previous_incoming_seconds)),
        (JoinSide::Next, room_seconds(next, next_outgoing_seconds)),
    ];
    if let Some((side, room)) = sides.into_iter().find(|(_, room)| duration_seconds > room + TIME_EPSILON_SECONDS) {
        return (Transition::HARD_CUT, Some(SkippedDefault { side, duration_seconds, room_seconds: room }));
    }
    (Transition::xfade(name, duration_seconds), None)
}

fn build_joins(pieces: &[Piece], clips_by_id: &HashMap<&str, &Clip>, defaults: &TransitionDefaults) -> Vec<Join> {
    let mut joins: Vec<Join> = Vec::new();
    for index in 1..pieces.len() {
        let previous = &pieces[index - 1];
        let next = &pieces[index];
        let override_ = transition_override(next, clips_by_id);
        let gap_seconds = (next.timeline_start_second - previous.timeline_end_second).max(0.0);
        let previous_incoming = joins.last().map_or(0.0, |join| join.transition.duration_seconds());
        let next_outgoing = pieces
            .get(index + 1)
            .map_or(0.0, |following| transition_override(following, clips_by_id).map_or(0.0, |transition| transition.duration_seconds()));
        let (transition, skipped_default) = resolve_join_transition(override_, previous, next, gap_seconds, previous_incoming, next_outgoing, defaults);
        joins.push(Join {
            key: segment_join_key(&next.video_id, &next.segment_key),
            video_id: next.video_id.clone(),
            segment_key: next.segment_key.clone(),
            from_piece_id: previous.id.clone(),
            to_piece_id: next.id.clone(),
            gap_seconds,
            transition,
            is_default: override_.is_none(),
            skipped_default,
        });
    }
    joins
}

/// `buildProgram`.
pub fn build_program(clips: &[Clip], defaults: &TransitionDefaults) -> Program {
    let clips_by_id: HashMap<&str, &Clip> = clips.iter().map(|clip| (clip.id(), clip)).collect();
    let (pieces, overlaps) = resolve_overlaps(collect_pieces(&sort_clips_by_timeline(clips)), &clips_by_id);
    let joins = build_joins(&pieces, &clips_by_id, defaults);
    let pieces_seconds: f64 = pieces.iter().map(|piece| piece.duration_seconds).sum();
    let transition_seconds: f64 = joins.iter().map(|join| join.transition.duration_seconds()).sum();
    Program { duration_seconds: pieces_seconds - transition_seconds, pieces, joins, overlaps, excluded_segments: collect_excluded_segments(clips) }
}

/// A piece whose transitions leave no untouched frame (`findInvalidTransitions`).
#[derive(Clone, Debug, PartialEq)]
pub struct InvalidTransition {
    pub piece_id: String,
    pub join_keys: Vec<String>,
}

/// `findInvalidTransitions`.
pub fn find_invalid_transitions(program: &Program) -> Vec<InvalidTransition> {
    program
        .pieces
        .iter()
        .enumerate()
        .filter_map(|(index, piece)| {
            let incoming = index.checked_sub(1).and_then(|index| program.joins.get(index));
            let outgoing = program.joins.get(index);
            let seconds = |join: Option<&Join>| join.map_or(0.0, |join| join.transition.duration_seconds());
            let leaves_untouched_frame = seconds(incoming) + seconds(outgoing) + piece.fps.frames_to_seconds(1) <= piece.duration_seconds + TIME_EPSILON_SECONDS;
            (!leaves_untouched_frame).then(|| InvalidTransition {
                piece_id: piece.id.clone(),
                join_keys: [incoming, outgoing].into_iter().flatten().filter(|join| join.transition.duration_seconds() > 0.0).map(|join| join.key.clone()).collect(),
            })
        })
        .collect()
}

/// What the program shows at a timeline second (`resolveProgramPosition`).
#[derive(Clone, Debug, PartialEq)]
pub enum ProgramPosition<'a> {
    Empty,
    Before { next_piece: &'a Piece },
    After { previous_piece: &'a Piece },
    Gap { previous_piece: &'a Piece, next_piece: &'a Piece, join: Option<&'a Join> },
    Piece { piece: &'a Piece, source_second: f64, next_piece: Option<&'a Piece>, join: Option<&'a Join> },
    /// Inside the last `d` seconds of a piece whose outgoing join lasts `d`; the incoming source
    /// advances from the next piece's start.
    Transition { piece: &'a Piece, source_second: f64, next_piece: &'a Piece, join: &'a Join, progress: f64, incoming_source_second: f64 },
}

/// `resolveProgramPosition`.
pub fn resolve_program_position(program: &Program, second: f64) -> ProgramPosition<'_> {
    let pieces = &program.pieces;
    let Some(first) = pieces.first() else { return ProgramPosition::Empty };
    if second < first.timeline_start_second {
        return ProgramPosition::Before { next_piece: first };
    }
    for (index, piece) in pieces.iter().enumerate() {
        let next_piece = pieces.get(index + 1);
        let join = program.joins.get(index);
        if second >= piece.timeline_end_second {
            if let Some(next) = next_piece.filter(|next| second < next.timeline_start_second) {
                return ProgramPosition::Gap { previous_piece: piece, next_piece: next, join };
            }
            continue;
        }
        let source_second = piece.source_start_second + (second - piece.timeline_start_second);
        let transition_seconds = join.map_or(0.0, |join| join.transition.duration_seconds());
        let window_start = piece.timeline_end_second - transition_seconds;
        if transition_seconds > 0.0 && second >= window_start {
            let next = next_piece.expect("a join leads to a next piece");
            let elapsed = second - window_start;
            return ProgramPosition::Transition {
                piece,
                source_second,
                next_piece: next,
                join: join.expect("a transition has a join"),
                progress: elapsed / transition_seconds,
                incoming_source_second: next.source_start_second + elapsed,
            };
        }
        return ProgramPosition::Piece { piece, source_second, next_piece, join };
    }
    ProgramPosition::After { previous_piece: pieces.last().expect("pieces are not empty") }
}

/// Which clip and source frame the preview shows at a timeline second (`resolvePreviewTarget`).
#[derive(Clone, Debug, PartialEq)]
pub struct PreviewTarget<'a> {
    pub video_id: &'a str,
    /// The piece, or excluded segment, whose clip is shown.
    pub piece: &'a Piece,
    pub source_second: f64,
    pub is_inside: bool,
    pub is_excluded: bool,
}

/// `resolvePreviewTarget`: inside a piece (its outgoing transition included) its own frame; inside
/// an excluded segment no piece covers, the segment's frame, flagged; before the first piece and
/// in gaps the upcoming piece's first frame; after the last piece its final frame.
pub fn resolve_preview_target(program: &Program, second: f64) -> Option<PreviewTarget<'_>> {
    fn target(piece: &Piece, source_second: f64, is_inside: bool, is_excluded: bool) -> PreviewTarget<'_> {
        PreviewTarget { video_id: &piece.video_id, piece, source_second, is_inside, is_excluded }
    }
    let position = resolve_program_position(program, second);
    if let ProgramPosition::Piece { piece, source_second, .. } | ProgramPosition::Transition { piece, source_second, .. } = position {
        return Some(target(piece, source_second, true, false));
    }
    if let Some(segment) = program.excluded_segments.iter().find(|segment| segment.timeline_start_second <= second && second < segment.timeline_end_second) {
        return Some(target(segment, segment.source_start_second + (second - segment.timeline_start_second), false, true));
    }
    match position {
        ProgramPosition::Gap { next_piece, .. } | ProgramPosition::Before { next_piece } => Some(target(next_piece, next_piece.source_start_second, false, false)),
        ProgramPosition::After { previous_piece } => Some(target(previous_piece, previous_piece.fps.frames_to_seconds(previous_piece.source_end_frame - 1), false, false)),
        _ => None,
    }
}

/// The clock that drives playback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaybackSource {
    Timeline,
    Video,
}

/// `pieceClock`: the video, or the timeline for a piece of a missing video, which has no file.
pub fn piece_clock(piece: &Piece) -> PlaybackSource {
    if piece.missing {
        PlaybackSource::Timeline
    } else {
        PlaybackSource::Video
    }
}

/// Where playback starts and which clock drives it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlaybackStart {
    pub source: PlaybackSource,
    pub second: f64,
}

/// `resolvePlaybackStart`: inside a piece its clock plays; in a gap or excluded part between
/// pieces playback skips to the next piece; before the first and after the last piece the timeline
/// clock plays the activity.
pub fn resolve_playback_start(program: &Program, second: f64) -> PlaybackStart {
    match resolve_program_position(program, second) {
        ProgramPosition::Piece { piece, .. } | ProgramPosition::Transition { piece, .. } => PlaybackStart { source: piece_clock(piece), second },
        ProgramPosition::Gap { next_piece, .. } => PlaybackStart { source: piece_clock(next_piece), second: next_piece.timeline_start_second },
        _ => PlaybackStart { source: PlaybackSource::Timeline, second },
    }
}

#[cfg(test)]
mod tests {
    use super::super::collection::build_timeline_program;
    use super::super::fixtures::*;
    use super::super::{SegmentEdits, VideoEdits};
    use super::*;
    use ovrley_core::encode::ffmpeg::transitions::transition_defaults;

    struct V {
        offset: f64,
        duration: f64,
        order: u32,
        cuts: Vec<(String, i64, SegmentEdits)>,
        start: SegmentEdits,
        fps: (u32, u32),
    }

    impl Default for V {
        fn default() -> Self {
            Self { offset: 0.0, duration: 60.0, order: 0, cuts: Vec::new(), start: SegmentEdits::INCLUDED, fps: (30, 1) }
        }
    }

    fn v(id: &'static str, spec: V) -> Clip {
        let video = video(TestVideo { id, duration_seconds: spec.duration, fps: spec.fps, import_order: spec.order, ..TestVideo::default() });
        let edits = VideoEdits { offset_seconds: spec.offset, ..cut_edits(&spec.cuts, spec.start) };
        clip(video, edits)
    }

    fn wipe_defaults() -> TransitionDefaults {
        TransitionDefaults { transition_type: "wipeleft".into(), duration_seconds: 2.0, ..transition_defaults().clone() }
    }

    fn wipe() -> Transition {
        Transition::xfade("wipeleft", 2.0)
    }

    fn fade(seconds: f64) -> Transition {
        Transition::new(TransitionKind::from_name("fade").unwrap(), seconds).unwrap()
    }

    fn with_transition(transition: Transition) -> SegmentEdits {
        SegmentEdits { transition_in: Some(transition), ..SegmentEdits::INCLUDED }
    }

    fn spans(program: &Program) -> Vec<(String, f64, f64, f64, f64)> {
        program
            .pieces
            .iter()
            .map(|piece| (piece.video_id.clone(), piece.source_start_second, piece.source_end_second, piece.timeline_start_second, piece.timeline_end_second))
            .collect()
    }

    #[test]
    fn joins_get_hard_cuts_or_the_default_transition() {
        let cases: Vec<(&str, Vec<Clip>, TransitionDefaults, Transition, f64)> = vec![
            ("camera split without gap", vec![v("a", V::default()), v("b", V { offset: 60.0, order: 1, ..V::default() })], wipe_defaults(), Transition::HARD_CUT, 120.0),
            ("gap at the split threshold", vec![v("a", V::default()), v("b", V { offset: 62.0, order: 1, ..V::default() })], wipe_defaults(), Transition::HARD_CUT, 120.0),
            ("gap above the threshold", vec![v("a", V::default()), v("b", V { offset: 70.0, order: 1, ..V::default() })], wipe_defaults(), wipe(), 118.0),
            ("hard default", vec![v("a", V::default()), v("b", V { offset: 70.0, order: 1, ..V::default() })], transition_defaults().clone(), Transition::HARD_CUT, 120.0),
            (
                "larger threshold",
                vec![v("a", V::default()), v("b", V { offset: 70.0, order: 1, ..V::default() })],
                TransitionDefaults { split_gap_threshold_seconds: 15.0, ..wipe_defaults() },
                Transition::HARD_CUT,
                120.0,
            ),
            ("default too long for a short piece", vec![v("a", V::default()), v("b", V { offset: 70.0, duration: 2.0, order: 1, ..V::default() })], wipe_defaults(), Transition::HARD_CUT, 62.0),
        ];
        for (name, clips, defaults, transition, duration) in cases {
            let program = build_program(&clips, &defaults);
            assert_eq!(program.joins.len(), 1, "{name}");
            let join = &program.joins[0];
            assert_eq!((join.key.as_str(), join.video_id.as_str(), join.segment_key.as_str(), join.transition, join.is_default), ("b:start", "b", "start", transition, true), "{name}");
            assert!((program.duration_seconds - duration).abs() < 1e-9, "{name}: {}", program.duration_seconds);
        }
    }

    #[test]
    fn an_override_wins_and_changes_the_output_duration() {
        let program = build_program(&[v("a", V::default()), v("b", V { offset: 60.0, order: 1, start: with_transition(fade(3.0)), ..V::default() })], &wipe_defaults());
        let join = &program.joins[0];
        assert_eq!((join.transition, join.is_default, join.gap_seconds), (fade(3.0), false, 0.0));
        assert_eq!(program.duration_seconds, 117.0);
    }

    #[test]
    fn default_durations_snap_to_the_joined_frames() {
        let ntsc = (30000, 1001);
        let clips = [v("a", V { fps: ntsc, ..V::default() }), v("b", V { offset: 70.0, order: 1, fps: ntsc, ..V::default() })];
        let program = build_program(&clips, &TransitionDefaults { duration_seconds: 1.01, ..wipe_defaults() });
        assert_eq!(program.joins[0].transition, Transition::xfade("wipeleft", 30.0 * 1001.0 / 30000.0));
    }

    #[test]
    fn an_excluded_middle_segment_leaves_two_pieces() {
        let cuts = vec![cut("c1", 9000, excluded()), cut("c2", 27000, SegmentEdits::INCLUDED)];
        let program = build_program(&[v("a", V { duration: 1200.0, cuts, ..V::default() })], &wipe_defaults());
        assert_eq!(spans(&program), [("a".to_string(), 0.0, 300.0, 0.0, 300.0), ("a".to_string(), 900.0, 1200.0, 900.0, 1200.0)]);
        let join = &program.joins[0];
        assert_eq!((join.key.as_str(), join.segment_key.as_str(), join.gap_seconds, join.transition), ("a:c2", "c2", 600.0, wipe()));
        assert_eq!(program.duration_seconds, 598.0);
        let excluded: Vec<_> = program.excluded_segments.iter().map(|piece| (piece.video_id.as_str(), piece.segment_key.as_str(), piece.timeline_start_second, piece.timeline_end_second)).collect();
        assert_eq!(excluded, [("a", "c1", 300.0, 900.0)]);
    }

    #[test]
    fn excluded_edges_trim_without_joins() {
        let cuts = vec![cut("c1", 150, SegmentEdits::INCLUDED), cut("c2", 1650, excluded())];
        let program = build_program(&[v("a", V { offset: 10.0, cuts, start: excluded(), ..V::default() })], &wipe_defaults());
        assert_eq!(spans(&program), [("a".to_string(), 5.0, 55.0, 15.0, 65.0)]);
        assert!(program.joins.is_empty());
    }

    #[test]
    fn a_wholly_excluded_video_leaves_no_piece_and_joins_its_neighbours() {
        let program = build_program(
            &[v("a", V::default()), v("b", V { offset: 60.0, duration: 10.0, order: 1, start: excluded(), ..V::default() }), v("c", V { offset: 70.0, order: 2, ..V::default() })],
            &wipe_defaults(),
        );
        assert_eq!(program.pieces.iter().map(|piece| piece.video_id.as_str()).collect::<Vec<_>>(), ["a", "c"]);
        assert_eq!(program.joins.iter().map(|join| (join.key.as_str(), join.gap_seconds, join.transition)).collect::<Vec<_>>(), [("c:start", 10.0, wipe())]);
        assert_eq!(program.excluded_segments.iter().map(|piece| (piece.video_id.as_str(), piece.segment_key.as_str())).collect::<Vec<_>>(), [("b", "start")]);
    }

    #[test]
    fn the_join_into_a_trimmed_video_belongs_to_its_first_kept_segment() {
        let start = SegmentEdits { excluded: true, transition_in: Some(fade(1.0)), skip_overlay: false };
        let program = build_program(&[v("a", V::default()), v("b", V { offset: 60.0, order: 1, start, cuts: vec![cut("c1", 300, SegmentEdits::INCLUDED)], ..V::default() })], &wipe_defaults());
        assert_eq!(program.joins.len(), 1);
        assert_eq!((program.joins[0].key.as_str(), program.joins[0].is_default), ("b:c1", true));
    }

    #[test]
    fn join_keys_stay_unique_with_a_clip_inside_an_excluded_segment() {
        let cuts = vec![cut("c1", 150, excluded()), cut("c2", 450, SegmentEdits::INCLUDED)];
        let program = build_program(&[v("b", V { duration: 20.0, cuts, ..V::default() }), v("a", V { offset: 8.0, duration: 4.0, order: 1, ..V::default() })], &wipe_defaults());
        assert_eq!(program.pieces.iter().map(|piece| piece.video_id.as_str()).collect::<Vec<_>>(), ["b", "a", "b"]);
        assert_eq!(program.joins.iter().map(|join| join.key.as_str()).collect::<Vec<_>>(), ["a:start", "b:c2"]);
        assert!(program.overlaps.is_empty());
    }

    #[test]
    fn the_earlier_clip_plays_to_its_end_and_the_later_continues() {
        let program = build_program(&[v("a", V::default()), v("b", V { offset: 50.0, order: 1, ..V::default() })], &wipe_defaults());
        assert_eq!(program.overlaps, [Overlap { earlier_video_id: "a".into(), later_video_id: "b".into(), start_second: 50.0, end_second: 60.0 }]);
        assert_eq!(spans(&program), [("a".to_string(), 0.0, 60.0, 0.0, 60.0), ("b".to_string(), 10.0, 60.0, 60.0, 110.0)]);
        assert_eq!((program.joins[0].gap_seconds, program.joins[0].transition), (0.0, Transition::HARD_CUT));
        let excluded_overlap = build_program(&[v("a", V { cuts: vec![cut("c1", 1500, excluded())], ..V::default() }), v("b", V { offset: 50.0, order: 1, ..V::default() })], &wipe_defaults());
        assert!(excluded_overlap.overlaps.is_empty());
    }

    #[test]
    fn a_snapped_ntsc_clip_does_not_overlap_and_a_whole_frame_does() {
        let ntsc = (30000, 1001);
        let first = || v("a", V { duration: 10.01, fps: ntsc, ..V::default() });
        let end = first().duration_seconds();
        let at = |offset| build_program(&[first(), v("b", V { offset, order: 1, fps: ntsc, ..V::default() })], &wipe_defaults()).overlaps.len();
        assert_eq!(at(end), 0);
        assert_eq!(at(end - 0.01), 0);
        assert_eq!(at(end - 1001.0 / 30000.0), 1);
    }

    #[test]
    fn invalid_transitions_leave_no_untouched_frame() {
        let program = build_program(
            &[
                v("a", V::default()),
                v("b", V { offset: 60.0, duration: 5.0, order: 1, start: with_transition(fade(3.0)), ..V::default() }),
                v("c", V { offset: 65.0, order: 2, start: with_transition(fade(3.0)), ..V::default() }),
            ],
            &wipe_defaults(),
        );
        assert_eq!(find_invalid_transitions(&program), [InvalidTransition { piece_id: program.pieces[1].id.clone(), join_keys: vec!["b:start".into(), "c:start".into()] }]);
    }

    #[test]
    fn positions_before_inside_between_and_after_the_pieces() {
        let program = build_program(&[v("a", V::default()), v("b", V { offset: 70.0, order: 1, ..V::default() })], &wipe_defaults());
        let (first, second, join) = (&program.pieces[0], &program.pieces[1], &program.joins[0]);
        assert_eq!(resolve_program_position(&program, -1.0), ProgramPosition::Before { next_piece: first });
        assert_eq!(resolve_program_position(&program, 10.0), ProgramPosition::Piece { piece: first, source_second: 10.0, next_piece: Some(second), join: Some(join) });
        assert_eq!(resolve_program_position(&program, 65.0), ProgramPosition::Gap { previous_piece: first, next_piece: second, join: Some(join) });
        assert_eq!(resolve_program_position(&program, 75.0), ProgramPosition::Piece { piece: second, source_second: 5.0, next_piece: None, join: None });
        assert_eq!(resolve_program_position(&program, 130.0), ProgramPosition::After { previous_piece: second });
        assert_eq!(
            resolve_program_position(&program, 59.5),
            ProgramPosition::Transition { piece: first, source_second: 59.5, next_piece: second, join, progress: 0.75, incoming_source_second: 1.5 }
        );
    }

    fn two_clips() -> Program {
        let videos = [simple("first", 10.0, 0), simple("second", 10.0, 1)];
        build_timeline_program(&videos, &edits_at(&videos, &[("first", 5.0), ("second", 30.0)]), None, transition_defaults()).program
    }

    #[test]
    fn the_preview_shows_the_upcoming_or_final_frame_outside_the_pieces() {
        let program = two_clips();
        let (first, second) = (&program.pieces[0], &program.pieces[1]);
        let expect = |at: f64, video_id: &str, piece: &Piece, source_second: f64, is_inside: bool| {
            assert_eq!(resolve_preview_target(&program, at), Some(PreviewTarget { video_id, piece, source_second, is_inside, is_excluded: false }), "at {at}");
        };
        expect(0.0, "first", first, 0.0, false);
        expect(7.5, "first", first, 2.5, true);
        expect(20.0, "second", second, 0.0, false);
        expect(31.0, "second", second, 1.0, true);
        expect(50.0, "second", second, 299.0 / 30.0, false);
    }

    #[test]
    fn an_excluded_segment_shows_its_frame_and_playback_skips_it() {
        let videos = [simple("first", 10.0, 0)];
        let mut edits = edits_at(&videos, &[("first", 5.0)]);
        let cut_edits = cut_edits(&[cut("c1", 60, excluded()), cut("c2", 120, SegmentEdits::INCLUDED)], SegmentEdits::INCLUDED);
        let first = edits.get_mut("first").unwrap();
        first.cuts = cut_edits.cuts;
        first.segments = cut_edits.segments;
        let program = build_timeline_program(&videos, &edits, None, transition_defaults()).program;
        assert_eq!(
            resolve_preview_target(&program, 8.0),
            Some(PreviewTarget { video_id: "first", piece: &program.excluded_segments[0], source_second: 3.0, is_inside: false, is_excluded: true })
        );
        assert_eq!(resolve_playback_start(&program, 8.0), PlaybackStart { source: PlaybackSource::Video, second: 9.0 });
    }

    #[test]
    fn playback_skips_gaps_and_plays_the_activity_outside_the_videos() {
        let program = two_clips();
        let start = |at| resolve_playback_start(&program, at);
        assert_eq!(start(0.0), PlaybackStart { source: PlaybackSource::Timeline, second: 0.0 });
        assert_eq!(start(7.5), PlaybackStart { source: PlaybackSource::Video, second: 7.5 });
        assert_eq!(start(20.0), PlaybackStart { source: PlaybackSource::Video, second: 30.0 });
        assert_eq!(start(50.0), PlaybackStart { source: PlaybackSource::Timeline, second: 50.0 });
    }
}
