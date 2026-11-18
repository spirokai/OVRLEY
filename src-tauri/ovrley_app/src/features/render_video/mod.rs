//! The render-video feature (Phase 7): the render dialog (`useRenderWorkflow`,
//! `useRenderVideoDialogState`, `useRenderVideoDerivedState`, `useBatchInspection`), its request,
//! codec grid and export review, and the render session.
//!
//! The dialog's hook state is [`RenderDialogState`]; its effects are the `render_dialog`
//! reconcile step ([`reconcile_render_dialog`]); its presentation is [`view::render_dialog_view`].

pub mod codecs;
pub mod request;
pub mod review;
pub mod session;
pub mod view;

use crate::features::undo_redo::HistoryMode;
use crate::i18n::{catalog, interpolate};
use crate::lib::file_dialog::{load_remembered_render_directory, open_directory_path, remember_accepted_render_output, save_single_path, DialogOptions};
use crate::lib::update_rate::{normalize_update_rate_for_fps, FpsMode};
use crate::lib::video_program::collection::{derive_compatibility, sort_clips_by_timeline, TimelineProgram};
use crate::lib::video_program::Clip;
use crate::store::services::{BatchServiceError, Services};
use crate::store::Store;
use codecs::{codec_selection, default_bitrate, default_quality, first_available_acceleration, first_available_mp4_codec, is_mp4_codec, visible_accelerations, Acceleration, OutputFormat, PlatformOs};
use ovrley_core::encode::ffmpeg::catalog::CompositeCodecId;
use ovrley_core::encode::quality::{EncodingQuality, QualityType};
use ovrley_core::encode::render_settings::{ExportMode, OutputMode};
use ovrley_core::error::{CoreError, RenderPathError};
use request::{detected_codecs, export_destination, normalize_render_output_path, render_request, render_settings_draft, render_transport, RenderRequest, RenderSettingsDraft, RenderTransport, ReviewedBatch};
use review::{mark_stale_rows, review_export, ExportReview, ReviewInputs, SourceInspection};
use session::{is_renderer_busy, RenderTarget};
use std::collections::{BTreeMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;

/// The preference that remembers the separate files' folder.
const LAST_BATCH_OUTPUT_DIR_KEY: &str = "last-batch-output-dir";

/// `renderDialogPhase`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DialogPhase {
    #[default]
    Closed,
    Confirm,
    /// A single render's progress.
    Progress,
}

/// `useBatchInspection`'s inspection: the clips' source paths, the session and each source's
/// result. Kept across openings while the sources stay the same.
#[derive(Debug, Default)]
pub struct RenderInspection {
    paths: Vec<String>,
    inspection_id: Option<String>,
    sources: BTreeMap<String, SourceInspection>,
    /// Sources submission found changed since inspection.
    stale: HashSet<String>,
    /// Bumped on every change; keys the review cache.
    revision: u64,
}

impl RenderInspection {
    fn complete(&self) -> bool {
        self.inspection_id.is_some() && self.paths.iter().all(|path| self.sources.contains_key(path))
    }
}

/// The review cache's key: the committed timeline, the inspection and the draft's fields the
/// review reads.
#[derive(Debug)]
struct ReviewKey {
    timeline: Rc<TimelineProgram>,
    inspection_revision: u64,
    output_mode: OutputMode,
    export_mode: ExportMode,
    codec: String,
    output_path: String,
    language: crate::i18n::Locale,
}

impl PartialEq for ReviewKey {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.timeline, &other.timeline)
            && (self.inspection_revision, self.output_mode, self.export_mode, &self.codec, &self.output_path, self.language)
                == (other.inspection_revision, other.output_mode, other.export_mode, &other.codec, &other.output_path, other.language)
    }
}

/// The dialog's hook state (Phase 7 D4).
#[derive(Debug, Default)]
pub struct RenderDialogState {
    pub phase: DialogPhase,
    pub draft: Option<RenderSettingsDraft>,
    /// The output path's error, kept under the field with the draft.
    pub path_error: Option<RenderPathError>,
    /// The existing output file the overwrite question asks about.
    pub pending_overwrite: Option<String>,
    /// The missing-videos notice's count; 0 hides it.
    pub missing_video_count: usize,
    pub fps_mode: FpsMode,
    pub inspection: RenderInspection,
    review: Option<(ReviewKey, ExportReview)>,
}

impl RenderDialogState {
    /// The dialog or one of its questions is open: app shortcuts, drops and history wait.
    pub fn is_open(&self) -> bool {
        self.phase != DialogPhase::Closed || self.missing_video_count > 0
    }

    /// The reviewed export, while the dialog has videos.
    pub fn review(&self) -> Option<&ExportReview> {
        self.review.as_ref().map(|(_, review)| review)
    }
}

fn text(store: &Store, key: &str, default: &'static str) -> String {
    catalog().t_or(store.i18n.language, key, default).to_string()
}

/// `formatRenderPathError`: a path error in the UI language.
pub fn format_render_path_error(store: &Store, error: &RenderPathError) -> String {
    let fields = serde_json::to_value(error).expect("path errors serialize");
    let reason = fields["reason"].as_str().expect("path errors carry their reason");
    let template = catalog().t_or(store.i18n.language, &format!("render-video.pathErrors.{reason}"), "");
    if template.is_empty() {
        return error.to_string();
    }
    let values: Vec<(String, String)> = fields
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(key, value)| value.as_str().map(|text| (key.clone(), text.to_string())))
        .collect();
    interpolate(template, &values.iter().map(|(key, value)| (key.as_str(), value.as_str())).collect::<Vec<_>>())
}

// --- derived inputs -----------------------------------------------------------------------------

/// The committed clips in timeline order (`sortVideosByTimeline(joinVideoEdits(…))`).
pub fn sorted_clips(store: &Store) -> Vec<Clip> {
    sort_clips_by_timeline(&store.player.timeline.timeline().committed.clips).into_iter().cloned().collect()
}

/// `deriveCompatibility(importedVideos).reference`: the videos' shared resolution and rate.
pub fn video_format(store: &Store) -> Option<(ovrley_core::media::video_edits::VideoResolution, ovrley_core::encode::fps::Fps)> {
    let clips = &store.player.timeline.timeline().committed.clips;
    derive_compatibility(&clips.iter().collect::<Vec<_>>()).reference
}

/// The videos' shared frame rate, as a number.
pub fn imported_video_fps(store: &Store) -> Option<f64> {
    video_format(store).map(|(_, fps)| {
        let (num, den) = fps.components();
        f64::from(num) / f64::from(den)
    })
}

/// `lockedVideoFps`: a combined file has the videos' shared rate; separate files keep each one's.
pub fn locked_video_fps(store: &Store, draft: &RenderSettingsDraft) -> Option<f64> {
    if draft.settings.output_mode == OutputMode::Separate || !store.video_import.has_videos() {
        None
    } else {
        imported_video_fps(store)
    }
}

/// `updateRateFps`: the rate the update rate divides.
pub fn update_rate_fps(store: &Store, draft: &RenderSettingsDraft) -> f64 {
    match locked_video_fps(store, draft) {
        Some(fps) if draft.settings.export_mode == ExportMode::Composite => fps.round(),
        _ => draft.settings.fps,
    }
}

/// `defaultBitrateForCodec`: the videos' size and rate, else the scene's size and the draft's.
pub fn default_bitrate_for(store: &Store, draft: &RenderSettingsDraft, codec: CompositeCodecId) -> u32 {
    match video_format(store).filter(|_| store.video_import.has_videos()) {
        Some((resolution, fps)) => {
            let (num, den) = fps.components();
            default_bitrate(resolution.width, resolution.height, f64::from(num) / f64::from(den), codec)
        }
        None => {
            let dimension = |key: &str| store.editor.config.scene.get(key).and_then(serde_json::Value::as_u64).unwrap_or(0) as u32;
            default_bitrate(dimension("width"), dimension("height"), draft.settings.fps, codec)
        }
    }
}

/// The default quality value of `codec` for the draft's quality type.
fn default_quality_value(store: &Store, draft: &RenderSettingsDraft, codec: &str) -> EncodingQuality {
    let (format, _) = codec_selection(codec).expect("MP4 defaults are asked for grid codecs");
    match draft.settings.quality {
        EncodingQuality::Quality(_) => EncodingQuality::Quality(default_quality(format)),
        EncodingQuality::Bitrate(_) => {
            let id = CompositeCodecId::from_external_name(codec).expect("grid MP4 codecs are composite codecs");
            EncodingQuality::Bitrate(f64::from(default_bitrate_for(store, draft, id)))
        }
    }
}

/// `selectedExportCodecAvailable`: the draft's codec sits in the grid at an available acceleration.
pub fn selected_codec_available(store: &Store, codec: &str) -> bool {
    codec_selection(codec).is_some_and(|(format, acceleration)| {
        visible_accelerations(format, PlatformOs::current(), detected_codecs(store)).iter().any(|option| option.acceleration == acceleration && option.available)
    })
}

/// `resolutionMismatch`: every output renders at the scene size, so the videos must match it.
pub fn resolution_mismatch(store: &Store) -> bool {
    if !store.video_import.has_videos() {
        return false;
    }
    let scene = &store.editor.config.scene;
    let (Some(width), Some(height)) = (scene.get("width").and_then(serde_json::Value::as_u64), scene.get("height").and_then(serde_json::Value::as_u64)) else {
        return false;
    };
    video_format(store).is_some_and(|(resolution, _)| u64::from(resolution.width) != width || u64::from(resolution.height) != height)
}

/// Video parts render through the batch service; the activity alone through a single render.
pub fn renders_video_parts(store: &Store) -> bool {
    store.render_dialog.draft.as_ref().is_some_and(|draft| render_transport(store, Some(draft)) == RenderTransport::Batch)
}

/// `renderStartDisabled` of the derived state: busy, the mismatch, or a codec of the other mode
/// (or an unavailable one).
pub fn start_blocked(store: &Store, draft: &RenderSettingsDraft) -> bool {
    let mp4 = is_mp4_codec(&draft.settings.codec);
    is_renderer_busy(store)
        || resolution_mismatch(store)
        || match draft.settings.export_mode {
            ExportMode::Composite => !mp4 || !selected_codec_available(store, &draft.settings.codec),
            ExportMode::Transparent => mp4 || codec_selection(&draft.settings.codec).is_none(),
        }
}

/// The reviewed export ready to submit (`batch.request`): the inspection complete and fresh, the
/// review with outputs and nothing blocking, a destination, and no export running.
pub fn ready_batch(store: &Store) -> Option<ReviewedBatch> {
    let dialog = &store.render_dialog;
    let draft = dialog.draft.as_ref()?;
    if dialog.phase != DialogPhase::Confirm || store.batch_render.batch_snapshot.is_some() || is_renderer_busy(store) {
        return None;
    }
    let inspection = &dialog.inspection;
    if !inspection.complete() || !inspection.stale.is_empty() {
        return None;
    }
    let review = dialog.review()?;
    let outputs = review.outputs.as_ref().filter(|outputs| !outputs.is_empty())?;
    export_destination(draft.settings.output_mode, &draft.output_path, store.batch_render.batch_output_folder.as_deref(), true)?;
    Some(ReviewedBatch { inspection_id: inspection.inspection_id.clone()?, outputs: outputs.clone(), offsets: review.offsets.clone() })
}

// --- opening and closing ------------------------------------------------------------------------

/// Why rendering is disabled (`renderTooltipContent`), `None` when it is not. The document always
/// exists (React's config starts as `DEFAULT_CONFIG`), so React's "load a template" reasons never
/// show and are not ported.
pub fn render_disabled_reason(store: &Store) -> Option<String> {
    if store.media.activity_summary.is_none() && !has_video_telemetry(store) {
        return Some(text(store, "render-video.loadAGpxfitActivityFirst", "Load a GPX/FIT activity first"));
    }
    if is_renderer_busy(store) {
        return Some(text(store, "render-video.renderingAlreadyInProgress", "Rendering already in progress"));
    }
    None
}

/// `hasVideoTelemetry`: a video's embedded telemetry is ready.
fn has_video_telemetry(store: &Store) -> bool {
    store.video_import.imported_videos.iter().any(|video| video.telemetry.activity().is_some())
}

/// `openRenderDialog`: ignored while rendering is disabled. With missing videos the notice opens
/// instead; otherwise the draft comes from the committed settings and the output path is core's
/// suggestion in the remembered folder.
pub fn open_render_dialog(store: &mut Store, services: &mut Services) {
    if render_disabled_reason(store).is_some() || store.render_dialog.is_open() {
        return;
    }
    let missing = store.video_import.imported_videos.iter().filter(|video| video.missing).count();
    if missing > 0 {
        store.render_dialog.missing_video_count = missing;
        return;
    }
    let draft = render_settings_draft(store, String::new());
    let suggested = load_remembered_render_directory(services)
        .and_then(|directory| services.render.suggest_output_path(draft.settings.export_mode.into(), directory.as_deref()));
    match suggested {
        Ok(output_path) => {
            let dialog = &mut store.render_dialog;
            dialog.draft = Some(RenderSettingsDraft { output_path, ..draft });
            dialog.path_error = None;
            dialog.pending_overwrite = None;
            dialog.phase = DialogPhase::Confirm;
        }
        Err(error) => store.media.set_error_message(error),
    }
}

/// `closeRenderDialog`: ignored while the renderer is busy; clears the batch results, so the next
/// opening shows the settings.
pub fn close_render_dialog(store: &mut Store) {
    if is_renderer_busy(store) {
        return;
    }
    store.batch_render.clear_batch_results();
    let dialog = &mut store.render_dialog;
    dialog.phase = DialogPhase::Closed;
    dialog.draft = None;
    dialog.path_error = None;
    dialog.pending_overwrite = None;
}

pub fn dismiss_missing_videos(store: &mut Store) {
    store.render_dialog.missing_video_count = 0;
}

// --- draft edits --------------------------------------------------------------------------------

/// Applies an edit to the draft; nothing changes while the renderer is busy.
fn edit_draft(store: &mut Store, edit: impl FnOnce(&Store, &mut RenderSettingsDraft)) {
    if is_renderer_busy(store) {
        return;
    }
    let Some(mut draft) = store.render_dialog.draft.take() else { return };
    edit(store, &mut draft);
    store.render_dialog.draft = Some(draft);
}

fn clear_output_review(store: &mut Store) {
    store.render_dialog.path_error = None;
    store.render_dialog.pending_overwrite = None;
}

/// `handleOutputModeChange`: written to the store at once (`setOutputMode`, recorded as an undo
/// step as zundo does); separate files turn the whole activity off.
pub fn set_output_mode(store: &mut Store, mode: OutputMode) {
    if is_renderer_busy(store) || store.render_dialog.draft.is_none() {
        return;
    }
    store.render_settings.output_mode = mode;
    edit_draft(store, |_, draft| {
        draft.settings.output_mode = mode;
        if mode == OutputMode::Separate {
            draft.render_full_activity = false;
        }
    });
}

/// `handleExportModeChange`: the path takes the mode's extension; full video turns the whole
/// activity off.
pub fn set_export_mode(store: &mut Store, mode: ExportMode) {
    if is_renderer_busy(store) {
        return;
    }
    clear_output_review(store);
    edit_draft(store, |_, draft| {
        draft.settings.export_mode = mode;
        if mode == ExportMode::Composite {
            draft.render_full_activity = false;
        }
        if !draft.output_path.is_empty() {
            draft.output_path = normalize_render_output_path(&draft.output_path, mode);
        }
    });
}

/// `handleOutputPathCommit`: the path takes the mode's extension.
pub fn set_output_path(store: &mut Store, path: String) {
    if is_renderer_busy(store) {
        return;
    }
    clear_output_review(store);
    edit_draft(store, |_, draft| {
        draft.output_path = if path.is_empty() { path } else { normalize_render_output_path(&path, draft.settings.export_mode) };
    });
}

/// `handleBrowse`: the save dialog at the current path.
pub fn browse_output_path(store: &mut Store, services: &mut Services) {
    let Some(draft) = store.render_dialog.draft.as_ref().filter(|draft| !draft.output_path.is_empty()) else { return };
    let kind: ovrley_core::output::RenderOutputKind = draft.settings.export_mode.into();
    let shown = save_single_path(
        services,
        &draft.output_path,
        kind.extension(),
        None,
        None,
        Box::new(|store, _, selected| {
            if let Some(path) = selected {
                set_output_path(store, path);
            }
        }),
    );
    if let Err(error) = shown {
        store.media.set_error_message(error);
    }
}

/// `pickOutputFolder`: the separate files' folder, remembered.
pub fn pick_output_folder(store: &mut Store, services: &mut Services) {
    let shown = open_directory_path(
        services,
        DialogOptions { default_path: None, last_directory_key: Some(LAST_BATCH_OUTPUT_DIR_KEY) },
        Box::new(|store, _, selected| {
            if let Some(folder) = selected {
                set_batch_output_folder(store, Some(folder));
            }
        }),
    );
    if let Err(error) = shown {
        store.media.set_error_message(error);
    }
}

/// `setBatchOutputFolder`.
pub fn set_batch_output_folder(store: &mut Store, folder: Option<String>) {
    if let Err(error) = store.batch_render.set_batch_output_folder(folder) {
        store.media.set_error_message(error);
    }
}

/// `handleOutputFormatChange`: keeps the acceleration where it is available for the new format,
/// else the first available one; an MP4 format resets the quality value to its default.
pub fn select_format(store: &mut Store, format: OutputFormat) {
    edit_draft(store, |store, draft| {
        let platform = PlatformOs::current();
        let codecs = detected_codecs(store);
        let current = codec_selection(&draft.settings.codec).map(|(_, acceleration)| acceleration);
        let acceleration = visible_accelerations(format, platform, codecs)
            .into_iter()
            .find(|option| Some(option.acceleration) == current && option.available)
            .map(|option| option.acceleration)
            .or_else(|| first_available_acceleration(format, platform, codecs));
        let Some(codec) = acceleration.and_then(|acceleration| format.codec(acceleration)) else { return };
        draft.settings.codec = codec.to_string();
        if format.group() == codecs::FormatGroup::Mp4 {
            draft.settings.quality = default_quality_value(store, draft, codec);
        }
    });
}

/// `handleAccelerationChange`.
pub fn select_acceleration(store: &mut Store, acceleration: Acceleration) {
    edit_draft(store, |_, draft| {
        let Some((format, _)) = codec_selection(&draft.settings.codec) else { return };
        if let Some(codec) = format.codec(acceleration) {
            draft.settings.codec = codec.to_string();
        }
    });
}

/// `handleQualityTypeChange`: the value resets to the codec's default quality or bitrate.
pub fn set_quality_type(store: &mut Store, quality_type: QualityType) {
    edit_draft(store, |store, draft| {
        // The quality tabs show only for MP4 codecs, which sit in the grid.
        let Some((format, _)) = codec_selection(&draft.settings.codec) else { return };
        draft.settings.quality = match quality_type {
            QualityType::Quality => EncodingQuality::Quality(default_quality(format)),
            QualityType::Bitrate => {
                let codec = CompositeCodecId::from_external_name(&draft.settings.codec).expect("grid MP4 codecs are composite codecs");
                EncodingQuality::Bitrate(f64::from(default_bitrate_for(store, draft, codec)))
            }
        };
    });
}

/// `handleQualityValueChange`: the slider's position; quality runs inverted.
pub fn set_quality_slider(store: &mut Store, position: f64) {
    edit_draft(store, |_, draft| {
        draft.settings.quality = match draft.settings.quality {
            EncodingQuality::Quality(_) => EncodingQuality::Quality(codecs::invert_quality_value(position).round() as u8),
            EncodingQuality::Bitrate(_) => EncodingQuality::Bitrate(position),
        };
    });
}

/// The frame rate select (`handleFpsModeChange`); the update rate is normalized for the new rate.
pub fn select_fps_mode(store: &mut Store, mode: &str) {
    if is_renderer_busy(store) {
        return;
    }
    let dialog = &mut store.render_dialog;
    let Some(draft) = dialog.draft.as_mut() else { return };
    if let Some(fps) = dialog.fps_mode.select(mode, draft.settings.fps) {
        set_draft_fps(draft, fps);
    }
}

/// The custom rate field (`handleCustomFpsChange`).
pub fn set_custom_fps(store: &mut Store, typed: f64) {
    if is_renderer_busy(store) {
        return;
    }
    let dialog = &mut store.render_dialog;
    let Some(draft) = dialog.draft.as_mut() else { return };
    let fps = dialog.fps_mode.custom(typed);
    set_draft_fps(draft, fps);
}

fn set_draft_fps(draft: &mut RenderSettingsDraft, fps: u32) {
    draft.settings.fps = f64::from(fps);
    draft.settings.widget_update_rate = normalize_update_rate_for_fps(f64::from(fps), f64::from(draft.settings.widget_update_rate));
}

pub fn set_update_rate(store: &mut Store, rate: u32) {
    edit_draft(store, |_, draft| draft.settings.widget_update_rate = rate);
}

pub fn set_render_full_activity(store: &mut Store, enabled: bool) {
    edit_draft(store, |_, draft| draft.render_full_activity = enabled);
}

// --- submission ---------------------------------------------------------------------------------

/// What a submission returned, typed for the dialog.
enum SubmitOutcome {
    Single(ovrley_core::render_jobs::execution::RenderAccepted),
    Batch(Box<ovrley_core::render_jobs::contracts::BatchAcceptance>),
    /// An existing output, not cleared for overwriting.
    Exists,
    PathError(RenderPathError),
    /// Sources changed since their inspection.
    Stale(Vec<String>),
    Failed(String),
}

fn single_outcome(result: Result<ovrley_core::render_jobs::execution::RenderAccepted, CoreError>) -> SubmitOutcome {
    match result {
        Ok(accepted) => SubmitOutcome::Single(accepted),
        Err(CoreError::OutputExists(_)) => SubmitOutcome::Exists,
        Err(CoreError::OutputInvalid(error)) => SubmitOutcome::PathError(error),
        Err(CoreError::OutputIo { path, source }) => SubmitOutcome::PathError(RenderPathError::output_io(path, source)),
        Err(error) => SubmitOutcome::Failed(error.to_string()),
    }
}

fn batch_outcome(result: Result<ovrley_core::render_jobs::contracts::BatchAcceptance, BatchServiceError>) -> SubmitOutcome {
    match result {
        Ok(acceptance) => SubmitOutcome::Batch(Box::new(acceptance)),
        Err(BatchServiceError::AlreadyExists { .. }) => SubmitOutcome::Exists,
        Err(BatchServiceError::OutputError { error }) => SubmitOutcome::PathError(error),
        Err(BatchServiceError::ReinspectionRequired { rejection, .. }) => SubmitOutcome::Stale(rejection.issues.into_iter().map(|issue| issue.source_id).collect()),
        Err(error) => SubmitOutcome::Failed(error.to_string()),
    }
}

/// `submitRender`: Start, or the overwrite question's Overwrite (`expected_path`: the path it
/// asked about, which must still be the draft's). The submission runs on a worker.
pub fn start_render(store: &mut Store, services: &mut Services, overwrite: bool, expected_path: Option<String>) {
    let Some(draft) = store.render_dialog.draft.clone() else { return };
    if is_renderer_busy(store) {
        return;
    }
    let transport = render_transport(store, Some(&draft));
    let review = match transport {
        RenderTransport::Batch => match ready_batch(store) {
            Some(review) => Some(review),
            None => return,
        },
        RenderTransport::Single => None,
    };
    // A single render and a combined export write one chosen file.
    let writes_file = transport == RenderTransport::Single || draft.settings.output_mode == OutputMode::Combined;
    if writes_file && draft.output_path.is_empty() {
        store.render_dialog.path_error = Some(RenderPathError::OutputFileRequired);
        return;
    }
    if expected_path.is_some_and(|expected| expected != draft.output_path) {
        store.render_dialog.pending_overwrite = None;
        return;
    }
    let (request, accepted) = match render_request(store, &draft, review.as_ref(), overwrite) {
        Ok(built) => built,
        Err(error) => {
            if transport == RenderTransport::Single {
                store.render_dialog.phase = DialogPhase::Closed;
            }
            store.media.set_error_message(error);
            return;
        }
    };
    store.media.render_submission_target = Some(match transport {
        RenderTransport::Single => RenderTarget::Single,
        RenderTransport::Batch => RenderTarget::Batch,
    });
    let render = Arc::clone(&services.render);
    let rasters = services.rasters.clone();
    let output_path = draft.output_path.clone();
    services.workers.spawn("render-submit", move || {
        let outcome = match request {
            RenderRequest::Single { config, activity, output_path, overwrite } => single_outcome(render.submit_single(&rasters, config, activity, &output_path, overwrite)),
            RenderRequest::Batch(request) => batch_outcome(render.submit_batch(&rasters, request)),
        };
        Box::new(move |store: &mut Store, services: &mut Services| finish_submission(store, services, outcome, accepted, output_path, writes_file))
    });
}

fn finish_submission(store: &mut Store, services: &mut Services, outcome: SubmitOutcome, accepted: ovrley_core::encode::render_settings::RenderSettings, output_path: String, writes_file: bool) {
    let single = store.media.render_submission_target == Some(RenderTarget::Single);
    store.media.render_submission_target = None;
    let commit = |store: &mut Store| {
        // `runWithoutEditorHistory`: earlier entries keep the old settings, as in React.
        store.render_settings = accepted;
        store.history.set_mode(HistoryMode::Skip);
        clear_output_review(store);
    };
    match outcome {
        SubmitOutcome::Single(accepted_render) => {
            let path = accepted_render.output_path.display().to_string();
            store.media.start_render_session(accepted_render.render_id, path.clone(), text(store, "render-video.startingRender", "Starting render..."));
            remember_accepted_render_output(services, &path);
            commit(store);
            store.render_dialog.phase = DialogPhase::Progress;
            // Progress published before the session existed in the store was dropped; read it.
            let progress = services.render.progress();
            session::apply_render_progress(store, services, progress);
        }
        SubmitOutcome::Batch(acceptance) => {
            let batch_id = acceptance.batch_id.clone();
            store.batch_render.accept_batch_snapshot(acceptance.snapshot);
            commit(store);
            if let Ok(snapshot) = services.render.batch_snapshot(&batch_id) {
                session::apply_batch_snapshot(store, snapshot);
            }
        }
        SubmitOutcome::Exists if writes_file => store.render_dialog.pending_overwrite = Some(output_path),
        SubmitOutcome::PathError(error) if writes_file => store.render_dialog.path_error = Some(error),
        SubmitOutcome::Stale(source_ids) if !single => {
            let inspection = &mut store.render_dialog.inspection;
            inspection.stale.extend(source_ids);
            inspection.revision += 1;
        }
        SubmitOutcome::Exists => store.media.set_error_message(text(store, "render-video.overwriteExistingFile", "Overwrite existing file?")),
        SubmitOutcome::PathError(error) => {
            let message = format_render_path_error(store, &error);
            store.media.set_error_message(message);
        }
        SubmitOutcome::Stale(_) => store.media.set_error_message(text(store, "render-video.reinspectionRequired", "Sources changed or inspection expired. Inspect again before starting.")),
        SubmitOutcome::Failed(message) => {
            if single {
                store.render_dialog.phase = DialogPhase::Closed;
            }
            store.media.set_error_message(message);
        }
    }
}

/// The overwrite question's Overwrite.
pub fn confirm_overwrite(store: &mut Store, services: &mut Services) {
    if let Some(path) = store.render_dialog.pending_overwrite.clone() {
        start_render(store, services, true, Some(path));
    }
}

pub fn cancel_overwrite(store: &mut Store) {
    store.render_dialog.pending_overwrite = None;
}

/// The dialog's Cancel during a render (`handleCancel`).
pub fn cancel_dialog_render(store: &mut Store, services: &mut Services) {
    let target = if renders_video_parts(store) { RenderTarget::Batch } else { RenderTarget::Single };
    session::cancel_render(store, services, target);
}

/// The header's PNG button (debug builds): the playhead's frame, rendered on a worker to a
/// transparent PNG in Downloads, then opened. It holds the renderer while it runs.
pub fn render_preview_frame(store: &mut Store, services: &mut Services) {
    if render_disabled_reason(store).is_some() || is_renderer_busy(store) {
        return;
    }
    let request = match request::preview_render_request(store) {
        Ok(request) => request,
        Err(error) => {
            store.media.set_error_message(error);
            return;
        }
    };
    store.media.render_submission_target = Some(RenderTarget::Preview);
    let render = Arc::clone(&services.render);
    let rasters = services.rasters.clone();
    services.workers.spawn("render-preview-frame", move || {
        let rendered = render.render_preview_frame(&rasters, request.config, &request.activity, request.second).and_then(|path| render.open_file(&path));
        Box::new(move |store: &mut Store, _: &mut Services| {
            store.media.render_submission_target = None;
            if let Err(error) = rendered {
                store.media.set_error_message(error);
            }
        })
    });
}

/// The header's actions (`ActionButtons`).
#[derive(Clone, Debug, PartialEq)]
pub struct HeaderActionsView {
    pub start_disabled: bool,
    /// Why Start Render is disabled; empty when it is not.
    pub start_tooltip: String,
    /// A single render runs: Start reads "Rendering frames...".
    pub rendering: bool,
    /// The PNG button, in debug builds.
    pub png_shown: bool,
}

pub fn header_actions_view(store: &Store) -> HeaderActionsView {
    let reason = render_disabled_reason(store);
    HeaderActionsView {
        start_disabled: reason.is_some(),
        start_tooltip: reason.unwrap_or_default(),
        rendering: store.media.rendering_video,
        png_shown: store.editor_shell.debug_mode_enabled,
    }
}

/// Closes the window. During a render (Q1: A, as React, without asking) the close cancels it and
/// waits on a worker for core's cleanup (`ffmpeg` stopped, the partial output removed) before the
/// window closes, so no hidden process outlives it.
pub fn close_window(store: &mut Store, services: &mut Services) {
    if !services.render.busy() {
        services.window.close();
        return;
    }
    if store.media.rendering_video {
        session::cancel_render(store, services, RenderTarget::Single);
    } else if store.batch_render.batch_snapshot.as_ref().is_some_and(|snapshot| snapshot.renderer_busy) {
        session::cancel_render(store, services, RenderTarget::Batch);
    } else {
        services.render.cancel();
    }
    let render = Arc::clone(&services.render);
    services.workers.spawn("render-close", move || {
        while render.busy() {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        Box::new(|_: &mut Store, services: &mut Services| services.window.close())
    });
}

// --- the reconcile step -------------------------------------------------------------------------

/// The `render_dialog` reconcile step: the progress phase ends with its render; the codec follows
/// the export mode and the update rate the effective rate; the inspection follows the clips; the
/// review follows its inputs.
pub fn reconcile_render_dialog(store: &mut Store, services: &mut Services) {
    if store.render_dialog.phase == DialogPhase::Progress && !store.media.rendering_video && store.media.render_progress.status.is_terminal() {
        store.render_dialog.phase = DialogPhase::Closed;
        store.render_dialog.draft = None;
    }
    reconcile_draft(store);
    reconcile_inspection(store, services);
    reconcile_review(store);
}

fn reconcile_draft(store: &mut Store) {
    let Some(draft) = store.render_dialog.draft.as_ref() else { return };
    let mut next = draft.clone();
    let mp4 = is_mp4_codec(&next.settings.codec);
    match next.settings.export_mode {
        // Transparent exports cannot keep an MP4 codec.
        ExportMode::Transparent if mp4 => next.settings.codec = "prores_ks".into(),
        ExportMode::Transparent => {}
        // Composite exports land on the first available MP4 codec, with its default value.
        ExportMode::Composite => {
            if !mp4 || !selected_codec_available(store, &next.settings.codec) {
                if let Some(codec) = first_available_mp4_codec(PlatformOs::current(), detected_codecs(store)) {
                    next.settings.quality = default_quality_value(store, &next, codec);
                    next.settings.codec = codec.to_string();
                }
            }
        }
    }
    let fps = update_rate_fps(store, &next);
    next.settings.widget_update_rate = normalize_update_rate_for_fps(fps, f64::from(next.settings.widget_update_rate));
    if &next != draft {
        store.render_dialog.draft = Some(next);
    }
}

/// `useBatchInspection`'s effect: while the dialog is open with videos and no results, each
/// source is inspected on a worker in a session; a new set of sources disposes the old session.
fn reconcile_inspection(store: &mut Store, services: &mut Services) {
    let open = store.render_dialog.phase == DialogPhase::Confirm && store.video_import.has_videos() && store.batch_render.batch_snapshot.is_none();
    if !open {
        return;
    }
    let mut paths: Vec<String> = sorted_clips(store).into_iter().map(|clip| clip.video.path).collect();
    paths.dedup();
    let inspection = &mut store.render_dialog.inspection;
    if inspection.inspection_id.is_some() && inspection.paths == paths {
        return;
    }
    if let Some(previous) = inspection.inspection_id.take() {
        services.render.dispose_inspection(&previous);
    }
    let inspection_id = services.render.create_inspection();
    *inspection = RenderInspection { paths: paths.clone(), inspection_id: Some(inspection_id.clone()), revision: inspection.revision + 1, ..RenderInspection::default() };
    // Requests are independent; core bounds the probes across sessions.
    for path in paths {
        let render = Arc::clone(&services.render);
        let id = inspection_id.clone();
        services.workers.spawn("render-inspect", move || {
            let result = render.inspect_source(&id, &path);
            Box::new(move |store: &mut Store, _: &mut Services| {
                let inspection = &mut store.render_dialog.inspection;
                if inspection.inspection_id.as_deref() == Some(id.as_str()) {
                    inspection.sources.insert(path, result);
                    inspection.revision += 1;
                }
            })
        });
    }
}

fn reconcile_review(store: &mut Store) {
    let Some(draft) = store.render_dialog.draft.as_ref().filter(|_| store.video_import.has_videos()) else {
        store.render_dialog.review = None;
        return;
    };
    let key = ReviewKey {
        timeline: Rc::clone(&store.player.timeline.timeline().committed),
        inspection_revision: store.render_dialog.inspection.revision,
        output_mode: draft.settings.output_mode,
        export_mode: draft.settings.export_mode,
        codec: draft.settings.codec.clone(),
        output_path: draft.output_path.clone(),
        language: store.i18n.language,
    };
    if store.render_dialog.review.as_ref().is_some_and(|(cached, _)| *cached == key) {
        return;
    }
    let clips = sorted_clips(store);
    let inspection = &store.render_dialog.inspection;
    let mut reviewed = review_export(&ReviewInputs {
        output_mode: key.output_mode,
        export_mode: key.export_mode,
        codec: &key.codec,
        clips: &clips,
        transition_defaults: &store.video_import.transition_defaults,
        sources: inspection.inspection_id.is_some().then_some(&inspection.sources),
        activity_timezone: store.media.activity_summary.as_ref().and_then(|summary| summary.timezone.as_deref()),
        locale: store.i18n.language,
        output_path: &key.output_path,
    });
    mark_stale_rows(&mut reviewed.rows, &inspection.stale, store.i18n.language);
    store.render_dialog.review = Some((key, reviewed));
}

#[cfg(test)]
mod tests;
