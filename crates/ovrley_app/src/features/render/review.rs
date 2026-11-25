//! The export review.
//!
//! Separate files render each video's program alone, so overlaps do not trim it. One combined
//! file renders the program of every video, with overlaps resolved. Transparent outputs join
//! their pieces with hard cuts.

use std::collections::{BTreeMap, HashMap, HashSet};
use ovrley_core::encode::ffmpeg::transitions::TransitionDefaults;
use ovrley_core::encode::render_settings::{ExportMode, OutputMode};
use ovrley_core::media::video_edits::Transition;
use ovrley_core::render_jobs::contracts::{BatchRenderOutput, BatchRenderPiece, BatchTransition};
use crate::dialogs::filename_from_selected_path;
use crate::features::render::codecs::is_qsv_full_codec;
use crate::format::{format_clock_duration, format_video_creation_time};
use crate::i18n::{catalog, interpolate, Locale};
use crate::video::Clip;
use crate::video::collection::{find_combined_export_issues, format_combined_export_issue};
use crate::video::program::{build_program, find_invalid_transitions, Piece, Program};
use crate::video::segments::is_video_excluded;

/// What one source's inspection returned: its native source id, or why it failed.
pub type SourceInspection = Result<String, String>;

/// How far a reviewed video is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowStatus {
    /// Its inspection has not come back.
    Checking,
    /// Ready to render.
    Pending,
    Blocked,
}

/// One reviewed video, in timeline order.
#[derive(Clone, Debug, PartialEq)]
pub struct ReviewRow {
    pub id: String,
    pub path: String,
    pub filename: String,
    pub offset_seconds: f64,
    /// The program pieces of this video in its output.
    pub pieces: Vec<Piece>,
    pub status: RowStatus,
    pub error: Option<String>,
    /// The inspected source's native id, once inspected.
    pub source_id: Option<String>,
    pub duration_label: Option<String>,
    pub creation_date_label: Option<String>,
    pub creation_time_label: Option<String>,
}

/// The review of an export: its rows, blocks and outputs.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ExportReview {
    pub rows: Vec<ReviewRow>,
    /// Why one combined file cannot be rendered.
    pub blockers: Vec<String>,
    /// The native outputs, once every video is reviewed and nothing blocks them.
    pub outputs: Option<Vec<BatchRenderOutput>>,
    /// The committed offsets of the outputs' sources, by source id.
    pub offsets: HashMap<String, f64>,
}

/// The review's inputs.
pub struct ReviewInputs<'a> {
    pub output_mode: OutputMode,
    pub export_mode: ExportMode,
    pub codec: &'a str,
    /// The clips in timeline order.
    pub clips: &'a [Clip],
    pub transition_defaults: &'a TransitionDefaults,
    /// The inspected sources by path; `None` before the inspection exists.
    pub sources: Option<&'a BTreeMap<String, SourceInspection>>,
    /// The external activity's timezone, which creation times are shown in.
    pub activity_timezone: Option<&'a str>,
    pub locale: Locale,
    /// The combined output file, which is its output id.
    pub output_path: &'a str,
}

fn text(locale: Locale, key: &str, default: &'static str) -> String {
    catalog().t_or(locale, key, default).to_string()
}

fn clip_name(clip: &Clip) -> String {
    filename_from_selected_path(&clip.video.path)
        .unwrap_or_default()
        .to_string()
}

/// The native output of a program: its pieces and the transitions of its joins.
fn native_output(
    id: &str,
    program: &Program,
    source_ids: &HashMap<&str, &str>,
    export_mode: ExportMode,
) -> BatchRenderOutput {
    let transition = |transition: &Transition| BatchTransition {
        transition_type: transition.kind().name().to_string(),
        duration_seconds: transition.duration_seconds(),
    };
    BatchRenderOutput {
        id: id.to_string(),
        pieces: program
            .pieces
            .iter()
            .map(|piece| BatchRenderPiece {
                source_id: source_ids[piece.video_id.as_str()].to_string(),
                start_frame: piece.source_start_frame as u64,
                end_frame: piece.source_end_frame as u64,
                skip_overlay: piece.skip_overlay,
            })
            .collect(),
        transitions: program
            .joins
            .iter()
            .map(|join| {
                if export_mode == ExportMode::Transparent {
                    transition(&Transition::HARD_CUT)
                } else {
                    transition(&join.transition)
                }
            })
            .collect(),
    }
}

/// Camera timestamps are external data: the date and the time as the creation time shows them.
/// The time is the trailing `hh:mm:ss`; without one the whole text is the date.
fn creation_labels(
    clip: &Clip,
    activity_timezone: Option<&str>,
    locale: Locale,
) -> (Option<String>, Option<String>) {
    let video = &clip.video;
    let formatted = format_video_creation_time(
        video.creation_time.as_deref(),
        video.time_source.as_deref(),
        activity_timezone,
        clip.edits.timezone_mode,
        locale,
    );
    let bytes = formatted.as_bytes();
    let has_time = bytes.len() >= 8 && {
        let time = &bytes[bytes.len() - 8..];
        time.iter().enumerate().all(|(index, byte)| {
            if index == 2 || index == 5 {
                *byte == b':'
            } else {
                byte.is_ascii_digit()
            }
        })
    };
    if has_time {
        let (date, time) = formatted.split_at(formatted.len() - 8);
        (Some(date.trim_end().to_string()), Some(time.to_string()))
    } else {
        (Some(formatted), None)
    }
}

/// Every video's row. A video renders at its committed offset; frames outside
/// the activity show raster widgets only.
fn review_rows(
    inputs: &ReviewInputs,
    pieces_by_video: &HashMap<&str, Vec<Piece>>,
) -> Vec<ReviewRow> {
    inputs
        .clips
        .iter()
        .map(|clip| {
            let pieces = pieces_by_video.get(clip.id()).cloned().unwrap_or_default();
            let mut row = ReviewRow {
                id: clip.id().to_string(),
                path: clip.video.path.clone(),
                filename: clip_name(clip),
                offset_seconds: clip.offset_seconds(),
                pieces,
                status: RowStatus::Checking,
                error: None,
                source_id: None,
                duration_label: None,
                creation_date_label: None,
                creation_time_label: None,
            };
            if row.pieces.is_empty() {
                row.status = RowStatus::Blocked;
                row.error = Some(if is_video_excluded(clip) {
                    text(
                        inputs.locale,
                        "render-video.excludedFromExport",
                        "Excluded from export",
                    )
                } else {
                    text(
                        inputs.locale,
                        "render-video.hiddenByOverlap",
                        "Hidden behind an overlapping video",
                    )
                });
                return row;
            }
            match inputs
                .sources
                .and_then(|sources| sources.get(&clip.video.path))
            {
                None => row,
                Some(Err(error)) => ReviewRow {
                    status: RowStatus::Blocked,
                    error: Some(error.clone()),
                    ..row
                },
                Some(Ok(source_id)) => {
                    let (creation_date_label, creation_time_label) =
                        creation_labels(clip, inputs.activity_timezone, inputs.locale);
                    ReviewRow {
                        status: RowStatus::Pending,
                        source_id: Some(source_id.clone()),
                        duration_label: Some(format_clock_duration(clip.duration_seconds())),
                        creation_date_label,
                        creation_time_label,
                        ..row
                    }
                }
            }
        })
        .collect()
}

fn combined_blockers(inputs: &ReviewInputs, program: &Program, rows: &[ReviewRow]) -> Vec<String> {
    let names = |ids: &HashSet<&str>| {
        inputs
            .clips
            .iter()
            .filter(|clip| ids.contains(clip.id()))
            .map(clip_name)
            .collect::<Vec<_>>()
            .join(", ")
    };
    let included: Vec<&Clip> = inputs
        .clips
        .iter()
        .filter(|clip| {
            program
                .pieces
                .iter()
                .any(|piece| piece.video_id == clip.id())
        })
        .collect();
    let mut blockers = Vec::new();
    if program.pieces.is_empty() {
        blockers.push(text(
            inputs.locale,
            "render-video.nothingToExport",
            "Every video part is excluded from export.",
        ));
    }
    // QSV full overlay keeps the source's display rotation, one per file.
    let keeps_rotation =
        inputs.export_mode == ExportMode::Composite && is_qsv_full_codec(inputs.codec);
    for issue in find_combined_export_issues(program, &included, keeps_rotation) {
        blockers.push(format_combined_export_issue(
            &issue,
            inputs.clips,
            inputs.locale,
        ));
    }
    if inputs.export_mode == ExportMode::Composite && !find_invalid_transitions(program).is_empty()
    {
        blockers.push(text(
            inputs.locale,
            "render-video.combinedInvalidTransitions",
            "Some transitions are longer than the video pieces they join.",
        ));
    }
    let unavailable: HashSet<&str> = rows
        .iter()
        .filter(|row| !row.pieces.is_empty() && row.status == RowStatus::Blocked)
        .map(|row| row.id.as_str())
        .collect();
    if !unavailable.is_empty() {
        let template = catalog().t_or(
            inputs.locale,
            "render-video.combinedUnavailable",
            "Some videos cannot be exported ({{names}}).",
        );
        blockers.push(interpolate(template, &[("names", &names(&unavailable))]));
    }
    blockers
}

/// The rows, the combined blockers, the outputs and their sources' offsets.
pub fn review_export(inputs: &ReviewInputs) -> ExportReview {
    let program_of = |clips: &[Clip]| build_program(clips, inputs.transition_defaults);
    let separate = inputs.output_mode == OutputMode::Separate;
    let programs: HashMap<&str, Program> = if separate {
        inputs
            .clips
            .iter()
            .map(|clip| (clip.id(), program_of(std::slice::from_ref(clip))))
            .collect()
    } else {
        HashMap::new()
    };
    let program = (!separate).then(|| program_of(inputs.clips));
    let program_for = |id: &str| {
        programs
            .get(id)
            .or(program.as_ref())
            .expect("each clip has its program")
    };
    let pieces_by_video: HashMap<&str, Vec<Piece>> = inputs
        .clips
        .iter()
        .map(|clip| {
            (
                clip.id(),
                program_for(clip.id())
                    .pieces
                    .iter()
                    .filter(|piece| piece.video_id == clip.id())
                    .cloned()
                    .collect(),
            )
        })
        .collect();
    let rows: Vec<ReviewRow> = review_rows(inputs, &pieces_by_video)
        .into_iter()
        .map(|row| {
            if separate
                && inputs.export_mode == ExportMode::Composite
                && row.status == RowStatus::Pending
                && !find_invalid_transitions(&programs[row.id.as_str()]).is_empty()
            {
                ReviewRow {
                    status: RowStatus::Blocked,
                    error: Some(text(
                        inputs.locale,
                        "render-video.invalidTransitions",
                        "A transition is longer than the video pieces it joins",
                    )),
                    ..row
                }
            } else {
                row
            }
        })
        .collect();
    let blockers = program.as_ref().map_or_else(Vec::new, |program| {
        combined_blockers(inputs, program, &rows)
    });
    let pending: Vec<&ReviewRow> = rows
        .iter()
        .filter(|row| row.status == RowStatus::Pending)
        .collect();
    let source_ids: HashMap<&str, &str> = pending
        .iter()
        .map(|row| {
            (
                row.id.as_str(),
                row.source_id
                    .as_deref()
                    .expect("a pending row is inspected"),
            )
        })
        .collect();
    let settled = rows.iter().all(|row| row.status != RowStatus::Checking);
    let outputs = (settled && blockers.is_empty()).then(|| match &program {
        None => pending
            .iter()
            .map(|row| {
                native_output(
                    &row.path,
                    &programs[row.id.as_str()],
                    &source_ids,
                    inputs.export_mode,
                )
            })
            .collect(),
        Some(program) => vec![native_output(
            inputs.output_path,
            program,
            &source_ids,
            inputs.export_mode,
        )],
    });
    let used: HashSet<&str> = outputs
        .iter()
        .flatten()
        .flat_map(|output| output.pieces.iter().map(|piece| piece.source_id.as_str()))
        .collect();
    let offsets = pending
        .iter()
        .filter_map(|row| {
            row.source_id
                .as_deref()
                .filter(|id| used.contains(id))
                .map(|id| (id.to_string(), row.offset_seconds))
        })
        .collect();
    ExportReview {
        rows,
        blockers,
        outputs,
        offsets,
    }
}

/// Rows whose source changed since inspection need a fresh one.
pub fn mark_stale_rows(rows: &mut [ReviewRow], stale_source_ids: &HashSet<String>, locale: Locale) {
    for row in rows {
        if row
            .source_id
            .as_ref()
            .is_some_and(|id| stale_source_ids.contains(id))
        {
            row.status = RowStatus::Blocked;
            row.error = Some(text(
                locale,
                "render-video.reinspectionRequired",
                "Sources changed or inspection expired. Inspect again before starting.",
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::video::collection::join_video_edits;
    use crate::video::fixtures::{
        cut, cut_edits, edits_at, excluded, metadata, video, TestVideo,
    };
    use crate::video::{create_imported_video, ImportedVideo};
    use ovrley_core::media::video_edits::{SegmentEdits, FIRST_SEGMENT_KEY};
    use ovrley_core::render_jobs::contracts::BatchTransition;

    fn defaults() -> TransitionDefaults {
        TransitionDefaults {
            transition_type: "wipeleft".into(),
            duration_seconds: 1.0,
            split_gap_threshold_seconds: 2.0,
        }
    }

    fn clips_of(
        videos: &[ImportedVideo],
        offsets: &[(&str, f64)],
        edit: impl FnOnce(&mut crate::video::VideoEditsMap),
    ) -> Vec<Clip> {
        let mut edits = edits_at(videos, offsets);
        edit(&mut edits);
        let mut clips = join_video_edits(videos, &edits);
        clips.sort_by(|left, right| {
            left.offset_seconds()
                .total_cmp(&right.offset_seconds())
                .then(left.video.import_order.cmp(&right.video.import_order))
        });
        clips
    }

    fn inspected(clips: &[Clip], failing: &[&str]) -> BTreeMap<String, SourceInspection> {
        clips
            .iter()
            .map(|clip| {
                (
                    clip.video.path.clone(),
                    if failing.contains(&clip.id()) {
                        Err("Unreadable".to_string())
                    } else {
                        Ok(format!("source-{}", clip.id()))
                    },
                )
            })
            .collect()
    }

    struct Review {
        output_mode: OutputMode,
        export_mode: ExportMode,
        codec: &'static str,
        failing: Vec<&'static str>,
        inspected: bool,
    }

    impl Default for Review {
        fn default() -> Self {
            Self {
                output_mode: OutputMode::Combined,
                export_mode: ExportMode::Composite,
                codec: "libx264",
                failing: Vec::new(),
                inspected: true,
            }
        }
    }

    fn review(clips: &[Clip], options: Review) -> ExportReview {
        let defaults = defaults();
        let sources = inspected(clips, &options.failing);
        review_export(&ReviewInputs {
            output_mode: options.output_mode,
            export_mode: options.export_mode,
            codec: options.codec,
            clips,
            transition_defaults: &defaults,
            sources: options.inspected.then_some(&sources),
            activity_timezone: None,
            locale: Locale::En,
            output_path: "C:/renders/joined.mp4",
        })
    }

    fn ride() -> ImportedVideo {
        video(TestVideo {
            id: "ride",
            path: "/videos/ride.mp4",
            duration_seconds: 60.0,
            ..TestVideo::default()
        })
    }

    fn climb() -> ImportedVideo {
        video(TestVideo {
            id: "climb",
            path: "/videos/climb.mp4",
            duration_seconds: 30.0,
            import_order: 1,
            ..TestVideo::default()
        })
    }

    fn piece(
        source_id: &str,
        start_frame: u64,
        end_frame: u64,
        skip_overlay: bool,
    ) -> BatchRenderPiece {
        BatchRenderPiece {
            source_id: source_id.into(),
            start_frame,
            end_frame,
            skip_overlay,
        }
    }

    fn transition(transition_type: &str, duration_seconds: f64) -> BatchTransition {
        BatchTransition {
            transition_type: transition_type.into(),
            duration_seconds,
        }
    }

    fn ids(review: &ExportReview) -> Vec<String> {
        review
            .outputs
            .iter()
            .flatten()
            .map(|output| output.id.clone())
            .collect()
    }

    fn row<'a>(review: &'a ExportReview, id: &str) -> &'a ReviewRow {
        review.rows.iter().find(|row| row.id == id).unwrap()
    }

    #[test]
    fn combines_the_program_into_one_output_joined_by_its_transitions() {
        let clips = clips_of(
            &[ride(), climb()],
            &[("ride", 0.0), ("climb", 70.0)],
            |edits| {
                edits.get_mut("climb").unwrap().segments.insert(
                    FIRST_SEGMENT_KEY.into(),
                    SegmentEdits {
                        skip_overlay: true,
                        ..SegmentEdits::INCLUDED
                    },
                );
            },
        );
        let reviewed = review(&clips, Review::default());
        assert!(reviewed.blockers.is_empty());
        let outputs = reviewed.outputs.clone().unwrap();
        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].id, "C:/renders/joined.mp4");
        let pieces: Vec<_> = outputs[0]
            .pieces
            .iter()
            .map(|piece| {
                (
                    piece.source_id.clone(),
                    piece.start_frame,
                    piece.end_frame,
                    piece.skip_overlay,
                )
            })
            .collect();
        assert_eq!(
            pieces,
            [
                ("source-ride".to_string(), 0, 1800, false),
                ("source-climb".to_string(), 0, 900, true)
            ]
        );
        // A 10 s gap is above the continuity threshold.
        assert_eq!(outputs[0].transitions, [transition("wipeleft", 1.0)]);
        assert_eq!(
            reviewed.offsets,
            HashMap::from([
                ("source-ride".to_string(), 0.0),
                ("source-climb".to_string(), 70.0)
            ])
        );
        // Transparent output joins with hard cuts.
        let transparent = review(
            &clips,
            Review {
                export_mode: ExportMode::Transparent,
                ..Review::default()
            },
        );
        assert_eq!(
            transparent.outputs.unwrap()[0].transitions,
            [transition("none", 0.0)]
        );
    }

    #[test]
    fn renders_separate_files_joined_across_their_excluded_segments() {
        let clips = clips_of(
            &[ride(), climb()],
            &[("ride", 0.0), ("climb", 30.0)],
            |edits| {
                let fade = SegmentEdits {
                    transition_in: Some(Transition::xfade("fade", 0.5)),
                    ..SegmentEdits::INCLUDED
                };
                let offset = edits["ride"].offset_seconds;
                let mut split = cut_edits(
                    &[cut("c1", 300, excluded()), cut("c2", 600, fade)],
                    SegmentEdits::INCLUDED,
                );
                split.offset_seconds = offset;
                edits.insert("ride".into(), split);
            },
        );
        // Overlapping videos render as separate files without trimming each other.
        let reviewed = review(
            &clips,
            Review {
                output_mode: OutputMode::Separate,
                ..Review::default()
            },
        );
        assert!(reviewed.blockers.is_empty());
        let outputs = reviewed.outputs.unwrap();
        assert_eq!(
            outputs
                .iter()
                .map(|output| output.id.as_str())
                .collect::<Vec<_>>(),
            ["/videos/ride.mp4", "/videos/climb.mp4"]
        );
        assert_eq!(
            outputs[0].pieces,
            [
                piece("source-ride", 0, 300, false),
                piece("source-ride", 600, 1800, false)
            ]
        );
        assert_eq!(outputs[0].transitions, [transition("fade", 0.5)]);
        assert_eq!(outputs[1].pieces, [piece("source-climb", 0, 900, false)]);
        assert!(outputs[1].transitions.is_empty());
    }

    #[test]
    fn an_encoder_keeping_the_display_rotation_joins_only_one_rotation() {
        // Coded portrait and rotated: the same display resolution as ride.
        let turned = create_imported_video(
            &ovrley_core::media::SourceVideoMetadata {
                rotation_degrees: Some(90),
                ..metadata("/videos/turned.mp4", 30.0, (30, 1), 1080, 1920)
            },
            "turned".into(),
            1,
        )
        .unwrap();
        let clips = clips_of(
            &[ride(), turned],
            &[("ride", 0.0), ("turned", 70.0)],
            |_| {},
        );
        assert!(review(&clips, Review::default()).blockers.is_empty());
        assert_eq!(
            review(&clips, Review { codec: "qsv_full_h264", ..Review::default() }).blockers,
            ["Videos are rotated differently (ride.mp4, turned.mp4), which this encoder cannot join into one file. Export separate files."]
        );
        assert!(review(
            &clips,
            Review {
                codec: "qsv_full_h264",
                output_mode: OutputMode::Separate,
                ..Review::default()
            }
        )
        .blockers
        .is_empty());
    }

    #[test]
    fn explains_every_reason_one_combined_file_cannot_be_rendered() {
        let wide = video(TestVideo {
            id: "wide",
            path: "/videos/wide.mp4",
            duration_seconds: 30.0,
            width: 3840,
            height: 2160,
            import_order: 2,
            ..TestVideo::default()
        });
        // Climb overlaps the end of ride and continues after it.
        let clips = clips_of(
            &[ride(), climb(), wide],
            &[("ride", 0.0), ("climb", 40.0), ("wide", 200.0)],
            |_| {},
        );
        let reviewed = review(
            &clips,
            Review {
                failing: vec!["climb"],
                ..Review::default()
            },
        );
        assert_eq!(reviewed.outputs, None);
        assert_eq!(
            reviewed.blockers,
            [
                "Videos overlap (ride.mp4, climb.mp4). Move a video or exclude the overlapping part from export.",
                "Videos differ in resolution or frame rate (wide.mp4). Remove them or export separate files.",
                "Some videos cannot be exported (climb.mp4).",
            ]
        );
        let everything_excluded = clips_of(&[ride()], &[("ride", 0.0)], |edits| {
            edits
                .get_mut("ride")
                .unwrap()
                .segments
                .insert(FIRST_SEGMENT_KEY.into(), excluded());
        });
        assert_eq!(
            review(&everything_excluded, Review::default()).blockers,
            ["Every video part is excluded from export."]
        );
    }

    #[test]
    fn a_video_wholly_behind_an_earlier_one_has_nothing_to_render() {
        let clips = clips_of(
            &[ride(), climb()],
            &[("ride", 0.0), ("climb", 10.0)],
            |_| {},
        );
        let reviewed = review(&clips, Review::default());
        assert_eq!(
            (
                row(&reviewed, "climb").status,
                row(&reviewed, "climb").error.as_deref()
            ),
            (
                RowStatus::Blocked,
                Some("Hidden behind an overlapping video")
            )
        );
    }

    #[test]
    fn blocks_transitions_longer_than_the_pieces_they_join() {
        let clips = clips_of(
            &[ride(), climb()],
            &[("ride", 0.0), ("climb", 70.0)],
            |edits| {
                edits.get_mut("climb").unwrap().segments.insert(
                    FIRST_SEGMENT_KEY.into(),
                    SegmentEdits {
                        transition_in: Some(Transition::xfade("fade", 40.0)),
                        ..SegmentEdits::INCLUDED
                    },
                );
                let long = SegmentEdits {
                    transition_in: Some(Transition::xfade("fade", 12.0)),
                    ..SegmentEdits::INCLUDED
                };
                edits.insert(
                    "ride".into(),
                    cut_edits(
                        &[cut("c1", 300, excluded()), cut("c2", 600, long)],
                        SegmentEdits::INCLUDED,
                    ),
                );
            },
        );
        assert_eq!(
            review(&clips, Review::default()).blockers,
            ["Some transitions are longer than the video pieces they join."]
        );
        let separate = review(
            &clips,
            Review {
                output_mode: OutputMode::Separate,
                ..Review::default()
            },
        );
        assert_eq!(
            (
                row(&separate, "ride").status,
                row(&separate, "ride").error.as_deref()
            ),
            (
                RowStatus::Blocked,
                Some("A transition is longer than the video pieces it joins")
            )
        );
        assert_eq!(ids(&separate), ["/videos/climb.mp4"]);
    }

    #[test]
    fn a_video_excluded_as_a_whole_renders_nothing_and_says_so() {
        let clips = clips_of(
            &[ride(), climb()],
            &[("ride", 0.0), ("climb", 70.0)],
            |edits| {
                edits
                    .get_mut("climb")
                    .unwrap()
                    .segments
                    .insert(FIRST_SEGMENT_KEY.into(), excluded());
            },
        );
        let separate = review(
            &clips,
            Review {
                output_mode: OutputMode::Separate,
                ..Review::default()
            },
        );
        assert_eq!(
            (
                row(&separate, "climb").status,
                row(&separate, "climb").error.as_deref()
            ),
            (RowStatus::Blocked, Some("Excluded from export"))
        );
        assert_eq!(ids(&separate), ["/videos/ride.mp4"]);
        let combined = review(&clips, Review::default());
        assert_eq!(
            combined.outputs.unwrap()[0]
                .pieces
                .iter()
                .map(|piece| piece.source_id.as_str())
                .collect::<Vec<_>>(),
            ["source-ride"]
        );
    }

    #[test]
    fn builds_no_outputs_while_videos_are_inspected_and_marks_stale_sources() {
        let clips = clips_of(&[ride()], &[("ride", 0.0)], |_| {});
        let checking = review(
            &clips,
            Review {
                inspected: false,
                ..Review::default()
            },
        );
        assert_eq!(
            (checking.outputs, checking.rows[0].status),
            (None, RowStatus::Checking)
        );
        let mut reviewed = review(&clips, Review::default());
        assert_eq!(reviewed.rows[0].duration_label.as_deref(), Some("01:00"));
        assert_eq!(
            (
                reviewed.rows[0].creation_date_label.as_deref(),
                reviewed.rows[0].creation_time_label.as_deref()
            ),
            (Some("Unknown"), None)
        );
        mark_stale_rows(
            &mut reviewed.rows,
            &HashSet::from(["source-ride".to_string()]),
            Locale::En,
        );
        assert_eq!(reviewed.rows[0].status, RowStatus::Blocked);
    }
}
