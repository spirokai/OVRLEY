//! Single and batch ingress, output review and per-video planning.
//! Owns the source-local clock and coverage for both export modes; pipelines
//! consume the resulting fixed plans without reinterpreting batch input.

use std::num::NonZeroU32;
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::contracts::{
    BatchDestination, BatchEncodingSettings, BatchExportMode, BatchOutputSelection,
    BatchRenderOutput, BatchRenderPiece, BatchTransition,
};
use super::inspection::CompositeSourceMetadata;
use super::inspection::{
    InspectionRejection, InspectionSourceSelection, InspectionValidation, VideoInspectionService,
};
use crate::activity::{
    build_dense_activity_report_for_timeline,
    schema::{DenseActivityReport, ParsedActivity},
};
use crate::encode::ffmpeg::assemble::{
    AssemblyAudio, AssemblyKind, AssemblySegment, AudioPiece, OutputAssembly, TransitionEncoding,
};
use crate::encode::ffmpeg::catalog::{
    CodecSelection, CompositeCodecId, CompositeFilterStackKind, TransparentCodecId,
};
use crate::encode::ffmpeg::settings::build_ffmpeg_settings;
use crate::encode::ffmpeg::transitions::{is_xfade_transition, NO_TRANSITION};
use crate::encode::fps::Fps;
use crate::encode::plan::CompositeRenderPlan;
use crate::encode::plan::FrameProductionPlan;
use crate::encode::plan::TransparentRenderPlan;
use crate::encode::quality::{validate_quality, EncodingQuality};
use crate::encode::video_timing::ActivityCoverage;
use crate::error::{CoreError, CoreResult};
use crate::media::prepared_video::InspectedVideoSource;
use crate::normalize::SceneConfig;
use crate::normalize::{
    validate_ffmpeg_config, validate_render_config_with_resources, validate_render_presentation,
    RenderConfig, ValidatedFfmpegConfig, ValidatedRenderConfig, ValidatedSceneConfig,
};
use crate::output::{plan_batch_output_targets, plan_file_output_target, RenderOutputKind};
use crate::raster::RasterResourceResolver;
use ovrley_widgets::model::scene::{ScenePresentationConfig, ValidatedScenePresentation};

/// Validated once at configuration/acceptance ingress; private fields keep
/// consumers from substituting malformed settings after validation.
pub(crate) struct ValidatedBatchEncoding {
    layout_fps: Fps,
    update_rate: NonZeroU32,
    codec: CodecSelection,
    quality: EncodingQuality,
    qsv_full_init_args: Vec<String>,
}

pub(crate) fn validate_batch_encoding(
    raw: &BatchEncodingSettings,
) -> CoreResult<ValidatedBatchEncoding> {
    let layout_fps = Fps::new(raw.fps, 1)?;
    let update_rate = NonZeroU32::new(raw.update_rate)
        .ok_or_else(|| CoreError::Config("Batch updateRate must be positive".into()))?;
    if raw.update_rate > raw.fps || raw.fps % raw.update_rate != 0 {
        return Err(CoreError::Config(
            "Batch updateRate must divide the selected FPS".into(),
        ));
    }
    let codec = CodecSelection::from_external_name(&raw.export_codec).ok_or_else(|| {
        CoreError::Config(format!("Unsupported batch codec: {}", raw.export_codec))
    })?;
    if !matches!(
        (raw.export_mode, codec),
        (BatchExportMode::Composite, CodecSelection::Composite(_))
            | (BatchExportMode::Transparent, CodecSelection::Transparent(_))
    ) {
        return Err(CoreError::Config(
            "Batch codec must match the selected export container".into(),
        ));
    }
    let quality = validate_quality(raw.quality_type, raw.quality_value)?;
    let qsv_full_init_args = raw.qsv_full_init_args.clone().unwrap_or_default();
    let needs_qsv = matches!(codec, CodecSelection::Composite(id)
        if id.metadata().filter_stack_kind == CompositeFilterStackKind::QsvFullOverlay);
    if needs_qsv
        && (qsv_full_init_args.is_empty()
            || qsv_full_init_args.iter().any(|arg| arg.trim().is_empty()))
    {
        return Err(CoreError::Config(
            "QSV full batch encoding requires detected hardware init args".into(),
        ));
    }
    if !needs_qsv && raw.qsv_full_init_args.is_some() {
        return Err(CoreError::Config(
            "QSV init args are only valid for QSV full encoding".into(),
        ));
    }
    Ok(ValidatedBatchEncoding {
        layout_fps,
        update_rate,
        codec,
        quality,
        qsv_full_init_args,
    })
}

/// The source window of one piece on its source's exact frame grid.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PieceWindow {
    pub trim_start: f64,
    pub duration: f64,
    frame_count: u64,
}

fn source_fps(source: &InspectedVideoSource) -> CoreResult<Fps> {
    Fps::new(
        source.metadata.fps_num.expect("inspected FPS numerator"),
        source.metadata.fps_den.expect("inspected FPS denominator"),
    )
}

/// Validates a piece against its source and derives its window. The source frame
/// count uses the same rule as the frontend program and the renderer.
pub(crate) fn piece_window(
    source: &InspectedVideoSource,
    piece: &BatchRenderPiece,
) -> CoreResult<PieceWindow> {
    let fps = source_fps(source)?;
    let total =
        fps.frame_count_for_duration(source.metadata.duration.expect("inspected source duration"))?;
    if piece.start_frame >= piece.end_frame || piece.end_frame > total {
        return Err(CoreError::Config(format!(
            "Piece frames [{}, {}) must lie within the {total} frames of {}",
            piece.start_frame, piece.end_frame, source.metadata.path
        )));
    }
    let trim_start = fps.seconds_at_frame(piece.start_frame);
    // A piece reaching the last frame ends where the source ends: that frame may be
    // shown only partly, and a whole-source piece renders exactly like the file.
    let end = if piece.end_frame == total {
        source.metadata.duration.expect("inspected source duration")
    } else {
        fps.seconds_at_frame(piece.end_frame)
    };
    Ok(PieceWindow {
        trim_start,
        duration: end - trim_start,
        frame_count: piece.end_frame - piece.start_frame,
    })
}

/// Output review and job preparation use the same exact frame-count rules.
fn video_output_work(
    source: &InspectedVideoSource,
    window: PieceWindow,
    codec: CodecSelection,
    layout_fps: Fps,
    update_rate: NonZeroU32,
) -> CoreResult<(u32, Fps)> {
    let (frames, fps) = match codec {
        CodecSelection::Composite(_) => (window.frame_count, source_fps(source)?),
        CodecSelection::Transparent(_) => {
            let layout = layout_fps.frame_count_for_duration(window.duration)?;
            (
                u64::from(FrameProductionPlan::decimated(layout, update_rate)?.count()),
                layout_fps.divided_by(update_rate)?,
            )
        }
    };
    let frames = u32::try_from(frames)
        .map_err(|_| CoreError::Encode("Batch output frame count exceeds u32".into()))?;
    Ok((frames, fps))
}

/// Validates the outputs against their destination before any source is touched:
/// every output holds pieces; a file destination takes exactly one output; a
/// source-named output holds pieces of one source.
pub(crate) fn validate_batch_outputs(
    outputs: &[BatchRenderOutput],
    destination: &BatchDestination,
) -> CoreResult<()> {
    if let Some(output) = outputs.iter().find(|output| output.pieces.is_empty()) {
        return Err(CoreError::Config(format!(
            "Batch output {} must contain at least one piece",
            output.id
        )));
    }
    match destination {
        BatchDestination::File { .. } if outputs.len() != 1 => Err(CoreError::Config(
            "A combined batch destination takes exactly one output".into(),
        )),
        BatchDestination::SourceNamed { .. } => match outputs.iter().find(|output| {
            output
                .pieces
                .iter()
                .any(|piece| piece.source_id != output.pieces[0].source_id)
        }) {
            Some(output) => Err(CoreError::Config(format!(
                "Separate batch output {} must contain pieces of one source",
                output.id
            ))),
            None => Ok(()),
        },
        BatchDestination::File { .. } => Ok(()),
    }
}

/// Distinct sources referenced by the outputs, in first-use order.
pub(crate) fn referenced_source_ids(outputs: &[BatchRenderOutput]) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    for piece in outputs.iter().flat_map(|output| &output.pieces) {
        if !ids.contains(&piece.source_id) {
            ids.push(piece.source_id.clone());
        }
    }
    ids
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchVideoOutputPlan {
    pub output_id: String,
    pub output_path: PathBuf,
    pub output_duration_seconds: f64,
    /// Frames rendered for the output (its progress work).
    pub planned_frames: u64,
    /// Frames of the finished output: the pieces minus their transitions.
    pub output_frames: u64,
    pub container_fps_num: u32,
    pub container_fps_den: u32,
}

#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum BatchPlanningResponse {
    Planned { plans: Vec<BatchVideoOutputPlan> },
    Rejected(InspectionRejection),
}

/// Public configuration seam: plans only fresh, session-owned descriptors.
pub fn plan_batch_configuration(
    inspection: &VideoInspectionService,
    selection: &BatchOutputSelection,
    encoding: &BatchEncodingSettings,
) -> CoreResult<BatchPlanningResponse> {
    validate_batch_outputs(&selection.outputs, &selection.destination)?;
    let encoding = validate_batch_encoding(encoding)?;
    let sources = match inspection.validate_sources(&InspectionSourceSelection {
        inspection_id: selection.inspection_id.clone(),
        source_ids: referenced_source_ids(&selection.outputs),
    }) {
        InspectionValidation::Valid(sources) => sources,
        InspectionValidation::Rejected(rejection) => {
            return Ok(BatchPlanningResponse::Rejected(rejection))
        }
    };
    let planned = encoding.plan_outputs(&selection.outputs, &sources, &selection.destination)?;
    let plans = selection
        .outputs
        .iter()
        .zip(planned)
        .map(|(output, planned)| {
            let (container_fps_num, container_fps_den) = planned.container_fps.components();
            BatchVideoOutputPlan {
                output_id: output.id.clone(),
                output_path: planned.target.path().to_path_buf(),
                output_duration_seconds: planned.duration,
                planned_frames: u64::from(planned.frames),
                output_frames: u64::from(planned.output_frames),
                container_fps_num,
                container_fps_den,
            }
        })
        .collect();
    Ok(BatchPlanningResponse::Planned { plans })
}

/// Submission-owned presentation and encoding, independent of any render clock.
pub(crate) struct ValidatedBatchTemplate {
    config: ValidatedRenderConfig<ValidatedScenePresentation>,
    encoding: ValidatedBatchEncoding,
}

/// One piece of a planned output: a window of one source.
pub(crate) struct PlannedPiece {
    pub source: std::sync::Arc<InspectedVideoSource>,
    /// Source frames `[start_frame, end_frame)`.
    pub start_frame: u64,
    pub end_frame: u64,
    pub window: PieceWindow,
    /// Output frames of this piece.
    pub frames: u32,
    pub skip_overlay: bool,
}

/// An `xfade` transition between two consecutive pieces of a composite output.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PlannedTransition {
    pub name: String,
    /// Output frames the transition spans (on the output frame grid).
    pub frames: u32,
}

/// Which part of its piece a rendered segment covers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SegmentRole {
    /// The first frames of a piece, blended into the incoming transition.
    Head,
    /// What a piece shows on its own: the whole piece between hard cuts.
    Body,
    /// The last frames of a piece, blended into the outgoing transition.
    Tail,
}

/// A separately rendered window of one piece.
pub(crate) struct PlannedSegment {
    pub piece: usize,
    pub role: SegmentRole,
    pub window: PieceWindow,
    pub frames: u32,
}

/// Reviewed and accepted jobs share the same output contract.
pub(crate) struct PlannedBatchOutput {
    pub target: crate::output::RenderOutputTarget,
    pub pieces: Vec<PlannedPiece>,
    /// One per join between consecutive pieces; None for a hard cut.
    pub transitions: Vec<Option<PlannedTransition>>,
    /// Rendered windows in order: per piece its head, body and tail.
    pub segments: Vec<PlannedSegment>,
    /// Frames rendered for every segment: the item's progress work.
    pub frames: u32,
    /// Frames of the joined output: every piece minus every transition.
    pub output_frames: u32,
    /// Duration of the joined output.
    pub duration: f64,
    pub container_fps: Fps,
}

impl PlannedBatchOutput {
    fn segment_index(&self, piece: usize, role: SegmentRole) -> usize {
        self.segments
            .iter()
            .position(|segment| segment.piece == piece && segment.role == role)
            .expect("planned segment")
    }
}

fn sum_frames(frames: impl Iterator<Item = u32>) -> CoreResult<u32> {
    frames
        .map(u64::from)
        .sum::<u64>()
        .try_into()
        .map_err(|_| CoreError::Encode("Batch output frame count exceeds u32".into()))
}

impl ValidatedBatchEncoding {
    /// Plans every requested output at its destination. The pieces of an output
    /// must join by stream copy: one display resolution, and for composite output
    /// one source FPS (and one rotation where the encoder keeps the display matrix).
    /// Pieces next to a transition render as head, body and tail segments.
    pub(crate) fn plan_outputs(
        &self,
        outputs: &[BatchRenderOutput],
        sources: &super::inspection::AcceptedInspectionSources,
        destination: &BatchDestination,
    ) -> CoreResult<Vec<PlannedBatchOutput>> {
        let source_of = |piece: &BatchRenderPiece| {
            sources
                .sources()
                .iter()
                .find(|source| source.source_id == piece.source_id)
                .cloned()
                .expect("validated sources cover every piece")
        };
        let kind = self.output_kind();
        let targets = match destination {
            BatchDestination::SourceNamed { directory } => plan_batch_output_targets(
                Path::new(directory),
                kind,
                &outputs
                    .iter()
                    .map(|output| PathBuf::from(&source_of(&output.pieces[0]).metadata.path))
                    .collect::<Vec<_>>(),
            )?,
            BatchDestination::File { path, overwrite } => vec![plan_file_output_target(
                path,
                kind,
                *overwrite,
                &sources
                    .sources()
                    .iter()
                    .map(|source| PathBuf::from(&source.metadata.path))
                    .collect::<Vec<_>>(),
            )?],
        };
        outputs
            .iter()
            .zip(targets)
            .map(|(output, target)| {
                let pieces = output
                    .pieces
                    .iter()
                    .map(|piece| {
                        let source = source_of(piece);
                        let window = piece_window(&source, piece)?;
                        let (frames, _) = video_output_work(
                            &source,
                            window,
                            self.codec,
                            self.layout_fps,
                            self.update_rate,
                        )?;
                        Ok(PlannedPiece {
                            source,
                            start_frame: piece.start_frame,
                            end_frame: piece.end_frame,
                            window,
                            frames,
                            skip_overlay: piece.skip_overlay,
                        })
                    })
                    .collect::<CoreResult<Vec<_>>>()?;
                self.check_joinable(&output.id, &pieces)?;
                let (_, container_fps) = video_output_work(
                    &pieces[0].source,
                    pieces[0].window,
                    self.codec,
                    self.layout_fps,
                    self.update_rate,
                )?;
                let transitions = self.plan_transitions(output, &pieces, container_fps)?;
                let segments = self.plan_segments(&pieces, &transitions)?;
                let transition_frames = sum_frames(
                    transitions
                        .iter()
                        .flatten()
                        .map(|transition| transition.frames),
                )?;
                let frames = sum_frames(segments.iter().map(|segment| segment.frames))?;
                let (num, den) = container_fps.components();
                Ok(PlannedBatchOutput {
                    target,
                    duration: pieces
                        .iter()
                        .map(|piece| piece.window.duration)
                        .sum::<f64>()
                        - f64::from(transition_frames) * f64::from(den) / f64::from(num),
                    output_frames: sum_frames(pieces.iter().map(|piece| piece.frames))?
                        - transition_frames,
                    pieces,
                    transitions,
                    segments,
                    frames,
                    container_fps,
                })
            })
            .collect()
    }

    /// Validates the joins of an output: one per pair of pieces, a hard cut or a
    /// catalogue `xfade` of at least one output frame that leaves every piece at
    /// least one frame of its own. Transparent outputs take hard cuts only.
    fn plan_transitions(
        &self,
        output: &BatchRenderOutput,
        pieces: &[PlannedPiece],
        container_fps: Fps,
    ) -> CoreResult<Vec<Option<PlannedTransition>>> {
        if output.transitions.len() + 1 != pieces.len() {
            return Err(CoreError::Config(format!(
                "Batch output {} must hold one transition per join of its {} pieces",
                output.id,
                pieces.len()
            )));
        }
        let invalid =
            |reason: String| CoreError::Config(format!("Batch output {}: {reason}", output.id));
        let transitions = output
            .transitions
            .iter()
            .map(|transition| {
                let BatchTransition {
                    transition_type,
                    duration_seconds,
                } = transition;
                if transition_type == NO_TRANSITION {
                    return if *duration_seconds == 0.0 {
                        Ok(None)
                    } else {
                        Err(invalid("a hard cut has a duration of 0 seconds".into()))
                    };
                }
                if !is_xfade_transition(transition_type) {
                    return Err(invalid(format!("unknown transition {transition_type}")));
                }
                if matches!(self.codec, CodecSelection::Transparent(_)) {
                    return Err(invalid(
                        "transparent outputs join pieces with hard cuts only".into(),
                    ));
                }
                if !duration_seconds.is_finite() || *duration_seconds <= 0.0 {
                    return Err(invalid("transition durations must be positive".into()));
                }
                let (num, den) = container_fps.components();
                let frames = (duration_seconds * f64::from(num) / f64::from(den)).round();
                if frames < 1.0 {
                    return Err(invalid(format!(
                        "the {transition_type} transition is shorter than one frame"
                    )));
                }
                Ok(Some(PlannedTransition {
                    name: transition_type.clone(),
                    frames: frames as u32,
                }))
            })
            .collect::<CoreResult<Vec<_>>>()?;
        let frames_of = |transition: Option<&Option<PlannedTransition>>| {
            transition
                .and_then(Option::as_ref)
                .map_or(0, |transition| transition.frames)
        };
        for (index, piece) in pieces.iter().enumerate() {
            let incoming = if index == 0 {
                0
            } else {
                frames_of(transitions.get(index - 1))
            };
            if u64::from(piece.frames)
                < u64::from(incoming) + u64::from(frames_of(transitions.get(index))) + 1
            {
                return Err(invalid(format!(
                    "the transitions around piece {} leave none of its {} frames untouched",
                    index + 1,
                    piece.frames
                )));
            }
        }
        Ok(transitions)
    }

    /// Splits pieces next to transitions into head, body and tail windows on
    /// their source frame grid (transitions exist only on composite outputs,
    /// whose output frames are source frames).
    fn plan_segments(
        &self,
        pieces: &[PlannedPiece],
        transitions: &[Option<PlannedTransition>],
    ) -> CoreResult<Vec<PlannedSegment>> {
        let frames_of = |index: Option<usize>| {
            index
                .and_then(|index| transitions.get(index))
                .and_then(Option::as_ref)
                .map_or(0, |transition| u64::from(transition.frames))
        };
        let mut segments = Vec::new();
        for (index, piece) in pieces.iter().enumerate() {
            let incoming = frames_of(index.checked_sub(1));
            let outgoing = frames_of(Some(index));
            if incoming == 0 && outgoing == 0 {
                segments.push(PlannedSegment {
                    piece: index,
                    role: SegmentRole::Body,
                    window: piece.window,
                    frames: piece.frames,
                });
                continue;
            }
            let body_start = piece.start_frame + incoming;
            let body_end = piece.end_frame - outgoing;
            for (role, start_frame, end_frame) in [
                (SegmentRole::Head, piece.start_frame, body_start),
                (SegmentRole::Body, body_start, body_end),
                (SegmentRole::Tail, body_end, piece.end_frame),
            ] {
                if start_frame == end_frame {
                    continue;
                }
                let window = piece_window(
                    &piece.source,
                    &BatchRenderPiece {
                        source_id: piece.source.source_id.clone(),
                        start_frame,
                        end_frame,
                        skip_overlay: piece.skip_overlay,
                    },
                )?;
                let (frames, _) = video_output_work(
                    &piece.source,
                    window,
                    self.codec,
                    self.layout_fps,
                    self.update_rate,
                )?;
                segments.push(PlannedSegment {
                    piece: index,
                    role,
                    window,
                    frames,
                });
            }
        }
        Ok(segments)
    }

    fn output_kind(&self) -> RenderOutputKind {
        match self.codec {
            CodecSelection::Composite(_) => RenderOutputKind::Composite,
            CodecSelection::Transparent(_) => RenderOutputKind::Transparent,
        }
    }

    fn check_joinable(&self, output_id: &str, pieces: &[PlannedPiece]) -> CoreResult<()> {
        let first = &pieces[0].source;
        let keeps_display_matrix = matches!(self.codec, CodecSelection::Composite(id)
            if id.metadata().filter_stack_kind == CompositeFilterStackKind::QsvFullOverlay);
        for piece in &pieces[1..] {
            let source = &piece.source;
            let reason = if source.display_resolution != first.display_resolution {
                Some("display resolution")
            } else if matches!(self.codec, CodecSelection::Composite(_))
                && source_fps(source)? != source_fps(first)?
            {
                Some("frame rate")
            } else if keeps_display_matrix
                && source
                    .metadata
                    .rotation_degrees
                    .unwrap_or(0)
                    .rem_euclid(360)
                    != first.metadata.rotation_degrees.unwrap_or(0).rem_euclid(360)
            {
                Some("rotation")
            } else {
                None
            };
            if let Some(reason) = reason {
                return Err(CoreError::Config(format!(
                    "Batch output {output_id} joins sources with a different {reason}: {} and {}",
                    first.metadata.path, source.metadata.path
                )));
            }
        }
        Ok(())
    }
}

impl ValidatedBatchTemplate {
    pub(crate) fn output_kind(&self) -> RenderOutputKind {
        self.encoding.output_kind()
    }

    /// How the rendered segments of a multi-piece output join into its target:
    /// each body in order, with an `xfade` of the outgoing tail and the incoming
    /// head at every transition. `segment_paths` follow `output.segments`.
    pub(crate) fn plan_assembly(
        &self,
        output: &PlannedBatchOutput,
        parts_directory: PathBuf,
        segment_paths: Vec<PathBuf>,
    ) -> OutputAssembly {
        let (num, den) = output.container_fps.components();
        let seconds = |frames: u32| f64::from(frames) * f64::from(den) / f64::from(num);
        let mut segments = Vec::new();
        for (index, transition) in output
            .transitions
            .iter()
            .map(Some)
            .chain([None])
            .enumerate()
        {
            segments.push(AssemblySegment::Part(
                segment_paths[output.segment_index(index, SegmentRole::Body)].clone(),
            ));
            if let Some(Some(transition)) = transition {
                segments.push(AssemblySegment::Transition {
                    outgoing: segment_paths[output.segment_index(index, SegmentRole::Tail)].clone(),
                    incoming: segment_paths[output.segment_index(index + 1, SegmentRole::Head)]
                        .clone(),
                    name: transition.name.clone(),
                    duration: seconds(transition.frames),
                    output: parts_directory.join(format!("transition-{index:04}.mp4")),
                });
            }
        }
        let kind = match self.encoding.codec {
            CodecSelection::Composite(id) => {
                let has_audio = output
                    .pieces
                    .iter()
                    .any(|piece| piece.source.metadata.has_audio);
                AssemblyKind::Composite {
                    hevc: id.metadata().encoder_id.is_hevc(),
                    // Joinable pieces share one rotation for this encoder.
                    display_rotation: (id.metadata().filter_stack_kind
                        == CompositeFilterStackKind::QsvFullOverlay)
                        .then(|| {
                            output.pieces[0]
                                .source
                                .metadata
                                .rotation_degrees
                                .map(|degrees| degrees.rem_euclid(360))
                        })
                        .flatten()
                        .filter(|degrees| *degrees != 0),
                    transition_encoding: TransitionEncoding::for_codec(
                        id,
                        self.encoding.quality,
                        output.container_fps,
                    ),
                    audio: has_audio.then(|| AssemblyAudio {
                        pieces: output
                            .pieces
                            .iter()
                            .map(|piece| AudioPiece {
                                source: piece
                                    .source
                                    .metadata
                                    .has_audio
                                    .then(|| PathBuf::from(&piece.source.metadata.path)),
                                trim_start: piece.window.trim_start,
                                duration: seconds(piece.frames),
                            })
                            .collect(),
                        crossfades: output
                            .transitions
                            .iter()
                            .map(|transition| {
                                transition
                                    .as_ref()
                                    .map_or(0.0, |transition| seconds(transition.frames))
                            })
                            .collect(),
                    }),
                }
            }
            CodecSelection::Transparent(_) => AssemblyKind::Transparent,
        };
        OutputAssembly {
            target: output.target.path().to_path_buf(),
            parts_directory,
            segments,
            frames: u64::from(output.output_frames),
            kind,
        }
    }
}

pub(crate) fn validate_batch_template(
    template: RenderConfig<ScenePresentationConfig>,
    encoding: ValidatedBatchEncoding,
    resources: Option<&dyn RasterResourceResolver>,
) -> CoreResult<ValidatedBatchTemplate> {
    if !template.extra.is_empty() || !template.scene.extra.is_empty() {
        return Err(CoreError::Config(
            "Batch template must contain only shared presentation fields".into(),
        ));
    }
    Ok(ValidatedBatchTemplate {
        config: validate_render_presentation(template, resources)?,
        encoding,
    })
}

pub enum VideoRenderModePlan {
    Composite {
        render: CompositeRenderPlan,
        /// Absent for a single render until its source is probed on the worker.
        source_metadata: Option<CompositeSourceMetadata>,
    },
    Transparent(TransparentRenderPlan),
}

/// No setters: each job owns its effective configuration and fixed frame grid.
pub struct PlannedVideoRender {
    pub(crate) config: ValidatedRenderConfig,
    pub(crate) mode: VideoRenderModePlan,
    sampling_fps: Fps,
    activity_offset: f64,
}

impl PlannedVideoRender {
    pub fn config(&self) -> &ValidatedRenderConfig {
        &self.config
    }
    pub fn mode(&self) -> &VideoRenderModePlan {
        &self.mode
    }
    pub fn planned_frames(&self) -> u32 {
        match &self.mode {
            VideoRenderModePlan::Composite { render: plan, .. } => plan.output_frame_count,
            VideoRenderModePlan::Transparent(plan) => plan.output_frame_count,
        }
    }
    /// Drops the source audio of a composite piece: a joined output encodes its
    /// audio once from the sources.
    pub(crate) fn without_audio(mut self) -> Self {
        if let VideoRenderModePlan::Composite {
            source_metadata: Some(metadata),
            ..
        } = &mut self.mode
        {
            metadata.has_audio = false;
        }
        self
    }
    pub fn prepare_activity(&self, activity: &ParsedActivity) -> CoreResult<DenseActivityReport> {
        let coverage = match &self.mode {
            VideoRenderModePlan::Composite { render: plan, .. } => &plan.coverage,
            VideoRenderModePlan::Transparent(plan) => &plan.coverage,
        };
        build_dense_activity_report_for_timeline(
            activity,
            &self.config,
            coverage.frame_timeline(self.sampling_fps, self.activity_offset),
        )
    }
}

/// Derives a single custom-range plan from validated submission inputs.
pub fn plan_single_render(
    raw: RenderConfig,
    activity_end: f64,
    resources: Option<&dyn RasterResourceResolver>,
) -> CoreResult<PlannedVideoRender> {
    let raw_scene = raw.scene.clone();
    let mut config = validate_render_config_with_resources(raw, resources)?;
    let (mode, sampling_fps, activity_offset) = if raw_scene.composite_video_path.is_some() {
        let render =
            derive_composite_render_plan(&raw_scene, &mut config.scene, Some(activity_end))?;
        let fps = render.overlay_pipe_fps;
        let offset = render.sync_offset;
        (
            VideoRenderModePlan::Composite {
                render,
                source_metadata: None,
            },
            fps,
            offset,
        )
    } else {
        let scene = &config.scene;
        let fps = Fps::new(scene.fps as u32, 1)?;
        let count = fps.frame_count_for_duration(scene.end - scene.start)?;
        (
            plan_transparent_render(
                scene,
                validate_ffmpeg_config(
                    raw_scene.ffmpeg,
                    CodecSelection::Transparent(TransparentCodecId::ProresKs),
                )?,
                count,
                fps.divided_by(scene.update_rate)?,
                ActivityCoverage {
                    start: scene.start,
                    end: scene.end,
                    blank_leading_frame_count: 0,
                    frame_count: count,
                },
            )?,
            fps,
            scene.start,
        )
    };
    Ok(PlannedVideoRender {
        config,
        mode,
        sampling_fps,
        activity_offset,
    })
}

/// Consumes timing and activity bounds validated by acceptance/job preparation.
///
/// The piece renders as a video of its window's duration that starts at activity
/// second `offset + window.trim_start` (`offset` is where source frame 0 sits).
/// Without an activity (`activity_end` is None) no frame is covered and only
/// rasters are kept.
#[allow(clippy::too_many_arguments)]
pub(crate) fn plan_batch_item(
    template: &ValidatedBatchTemplate,
    source: &InspectedVideoSource,
    window: PieceWindow,
    offset: f64,
    skip_overlay: bool,
    activity_end: Option<f64>,
    disabled_laps: &[usize],
    output_frame_count: u32,
    container_fps: Fps,
) -> CoreResult<PlannedVideoRender> {
    let duration = window.duration;
    let piece_offset = offset + window.trim_start;
    let encoding = &template.encoding;
    let update_rate = encoding.update_rate;
    let sampling_fps = match encoding.codec {
        CodecSelection::Composite(_) => container_fps.divided_by(update_rate)?,
        CodecSelection::Transparent(_) => encoding.layout_fps,
    };
    let layout_frame_count = sampling_fps.frame_count_for_duration(duration)?;
    let coverage = match activity_end {
        Some(activity_end) => ActivityCoverage::for_video(
            duration,
            piece_offset,
            activity_end,
            sampling_fps,
            layout_frame_count,
        )?,
        None => ActivityCoverage::without_activity(duration, layout_frame_count),
    };
    let mut presentation = template.config.scene.clone();
    presentation.width = source.display_resolution.width as u32;
    presentation.height = source.display_resolution.height as u32;
    let scene = ValidatedSceneConfig {
        presentation,
        start: coverage.start,
        end: coverage.end,
        fps: sampling_fps.as_f64(),
        update_rate: if matches!(encoding.codec, CodecSelection::Composite(_)) {
            NonZeroU32::MIN
        } else {
            update_rate
        },
        export_start_seconds: piece_offset,
        custom_export_range_active: Some(true),
        disabled_laps: disabled_laps.to_vec(),
    };
    let mut config = template.config.clone().with_scene(scene);
    if skip_overlay || activity_end.is_none() {
        config.backdrops.clear();
        config.labels.clear();
        config.values.clear();
        config.course_plots.clear();
        config.elevation_plots.clear();
        config.diving_profiles.clear();
        config.rolling_charts.clear();
    }
    let mode = match encoding.codec {
        CodecSelection::Composite(requested_codec_id) => VideoRenderModePlan::Composite {
            render: CompositeRenderPlan {
                video_path: PathBuf::from(&source.metadata.path),
                quality: encoding.quality,
                sync_offset: piece_offset,
                trim_start: window.trim_start,
                render_duration: duration,
                update_rate,
                source_fps: container_fps,
                overlay_pipe_fps: sampling_fps,
                frames: FrameProductionPlan::new(layout_frame_count, NonZeroU32::MIN)?,
                overlay_frame_count: layout_frame_count,
                output_frame_count,
                coverage,
                requested_codec_id,
                qsv_full_init_args: encoding.qsv_full_init_args.clone(),
            },
            source_metadata: Some(CompositeSourceMetadata::from_source(&source.metadata)),
        },
        CodecSelection::Transparent(_) => plan_transparent_render(
            &config.scene,
            ValidatedFfmpegConfig {
                codec: encoding.codec,
                ..ValidatedFfmpegConfig::default()
            },
            layout_frame_count,
            container_fps,
            coverage,
        )?,
    };
    Ok(PlannedVideoRender {
        config,
        mode,
        sampling_fps,
        activity_offset: piece_offset,
    })
}

fn plan_transparent_render(
    scene: &ValidatedSceneConfig,
    ffmpeg: ValidatedFfmpegConfig,
    layout_frame_count: u64,
    container_fps: Fps,
    coverage: ActivityCoverage,
) -> CoreResult<VideoRenderModePlan> {
    if !scene.presentation.width.is_multiple_of(2) || !scene.presentation.height.is_multiple_of(2) {
        return Err(CoreError::Config(
            "Transparent video dimensions must be even".into(),
        ));
    }
    let frames = FrameProductionPlan::decimated(layout_frame_count, scene.update_rate)?;
    Ok(VideoRenderModePlan::Transparent(TransparentRenderPlan {
        frames,
        ffmpeg: build_ffmpeg_settings(&ffmpeg)?,
        layout_frame_count: layout_frame_count as u32,
        output_frame_count: frames.count(),
        update_rate: scene.update_rate,
        container_fps: container_fps.ffmpeg_arg(),
        coverage,
    }))
}

const COMPOSITE_ACTIVITY_DURATION_SLACK_SECONDS: f64 = 0.25;

/// Validates composite render fields and derives timing/FPS values.
///
/// Required fields fail before dense activity is built, while optional fields
/// receive standard defaults.
pub fn derive_composite_render_plan(
    raw: &SceneConfig,
    scene: &mut crate::normalize::ValidatedSceneConfig,
    activity_end: Option<f64>,
) -> CoreResult<CompositeRenderPlan> {
    let video_path = raw
        .composite_video_path
        .as_ref()
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .ok_or_else(|| {
            CoreError::Config("scene.composite_video_path required for composite render".into())
        })?;
    let quality_type = raw.quality_type.ok_or_else(|| {
        CoreError::Config("scene.qualityType required for composite render".into())
    })?;
    let quality_value = raw.quality_value.ok_or_else(|| {
        CoreError::Config("scene.qualityValue required for composite render".into())
    })?;
    let quality = validate_quality(quality_type, quality_value)?;
    let ffmpeg = validate_ffmpeg_config(
        raw.ffmpeg.clone(),
        CodecSelection::Composite(CompositeCodecId::SoftwareH264),
    )?;
    let fps_num = raw.composite_video_fps_num.ok_or_else(|| {
        CoreError::Config("scene.composite_video_fps_num required for composite render".into())
    })?;
    let fps_den = raw.composite_video_fps_den.ok_or_else(|| {
        CoreError::Config("scene.composite_video_fps_den required for composite render".into())
    })?;
    let source_fps = Fps::new(fps_num, fps_den)?;
    let video_duration = raw.composite_video_duration.ok_or_else(|| {
        CoreError::Config("scene.composite_video_duration required for composite render".into())
    })?;
    if !video_duration.is_finite() || video_duration <= 0.0 {
        return Err(CoreError::Config(format!(
            "scene.composite_video_duration must be greater than zero: {video_duration}"
        )));
    }

    let sync_offset = scene.export_start_seconds;
    let trim_start = raw.composite_video_trim_start.ok_or_else(|| {
        CoreError::Config("scene.composite_video_trim_start required for composite render".into())
    })?;
    if !trim_start.is_finite() || trim_start < 0.0 {
        return Err(CoreError::Config(format!(
            "scene.composite_video_trim_start must be zero or greater: {trim_start}"
        )));
    }
    if trim_start >= video_duration {
        return Err(CoreError::Config(format!(
            "scene.composite_video_trim_start ({trim_start}) must be less than scene.composite_video_duration ({video_duration})"
        )));
    }

    let update_rate =
        std::num::NonZeroU32::new(raw.composite_widget_update_rate.ok_or_else(|| {
            CoreError::Config(
                "scene.composite_widget_update_rate required for composite render".into(),
            )
        })?)
        .ok_or_else(|| {
            CoreError::Config("scene.composite_widget_update_rate must be at least 1".into())
        })?;
    let overlay_pipe_fps = source_fps.divided_by(update_rate)?;
    let mut render_duration = raw
        .composite_render_duration
        .unwrap_or(video_duration - trim_start);
    if !render_duration.is_finite() || render_duration <= 0.0 {
        return Err(CoreError::Config(format!(
            "scene.composite_render_duration must be greater than zero: {render_duration}"
        )));
    }
    if let Some(activity_end) = activity_end {
        if !activity_end.is_finite() || activity_end < 0.0 {
            return Err(CoreError::Config(format!(
                "Composite activity end must be finite and zero or greater: {activity_end}"
            )));
        }
        let max_render_duration = activity_end - sync_offset;
        let overrun = render_duration - max_render_duration;
        if sync_offset >= 0.0 && max_render_duration > 0.0 {
            let ends_just_after_activity =
                overrun > 0.0 && overrun <= COMPOSITE_ACTIVITY_DURATION_SLACK_SECONDS;
            if ends_just_after_activity {
                render_duration = max_render_duration;
            }
        }
        let overlap_start = sync_offset.max(0.0);
        let overlap_end = activity_end.min(sync_offset + render_duration);
        // A video outside the activity samples none of it and renders rasters only.
        if overlap_end > overlap_start {
            scene.start = overlap_start;
            scene.end = overlap_end;
        } else {
            scene.start = 0.0;
            scene.end = activity_end;
        }
    } else {
        scene.start = sync_offset.max(0.0);
        scene.end = scene.start + render_duration;
    }
    let requested_codec_id = match ffmpeg.codec {
        CodecSelection::Composite(codec_id) => codec_id,
        CodecSelection::Transparent(codec_id) => {
            return Err(CoreError::Config(format!(
                "Transparent codec '{}' cannot be used for a composite render",
                codec_id.metadata().profile_name
            )))
        }
    };

    scene.fps = overlay_pipe_fps.as_f64();
    scene.update_rate = std::num::NonZeroU32::MIN;
    let overlay_frame_count = overlay_pipe_fps.frame_count_for_duration(render_duration)?;
    let coverage = ActivityCoverage::for_video(
        render_duration,
        sync_offset,
        scene.end,
        overlay_pipe_fps,
        overlay_frame_count,
    )?;
    let output_frame_count = u32::try_from(source_fps.frame_count_for_duration(render_duration)?)
        .map_err(|_| {
        CoreError::Encode("Composite output frame count exceeds u32".to_string())
    })?;

    Ok(CompositeRenderPlan {
        frames: crate::encode::plan::FrameProductionPlan::new(
            overlay_frame_count,
            std::num::NonZeroU32::MIN,
        )?,
        video_path: PathBuf::from(video_path),
        quality,
        sync_offset,
        trim_start,
        render_duration,
        update_rate,
        source_fps,
        overlay_pipe_fps,
        overlay_frame_count,
        output_frame_count,
        coverage,
        requested_codec_id,
        qsv_full_init_args: ffmpeg.qsv_full_init_args,
    })
}

#[cfg(test)]
mod composite_timing_tests {
    use super::*;

    #[test]
    fn counts_negative_lead_in_frames_at_fractional_offsets() {
        let fps = Fps::new(30, 1).expect("valid fps");

        for (offset, expected) in [(-0.01, 1), (-5.0, 150), (-5.01, 151)] {
            let coverage = ActivityCoverage::for_video(30.0, offset, 120.0, fps, 900).unwrap();
            assert_eq!(coverage.blank_leading_frame_count, expected);
        }
    }
}
