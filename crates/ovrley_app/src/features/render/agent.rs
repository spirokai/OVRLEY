//! What agents do with the render dialog: read the options, start a render as the dialog's Start
//! does (the window shows it), follow its progress (start, then wait) and cancel it. `ovrley_mcp`
//! hands its tool calls to these.

use std::num::NonZeroU32;

use ovrley_core::encode::settings::{QUALITY_SLIDER_MAX, QUALITY_SLIDER_MIN};
use ovrley_core::jobs::contracts::{BatchItemOutcome, BatchPhase};
use serde_json::{json, Value};
use crate::features::render::{close_render_dialog, format_render_path_error, imported_video_fps, locked_video_fps, open_render_dialog, ready_batch, reconcile_draft, render_disabled_reason, resolution_mismatch, select_acceleration, select_format, set_batch_output_folder, set_custom_fps, set_export_mode, set_output_mode, set_output_path, set_quality_type, set_render_full_activity, set_update_rate, start_blocked, start_render, text, update_rate_fps, view, DialogPhase, EncodingQuality, ExportMode, OutputMode, QualityType};
use crate::features::render::codecs::{codec_selection, default_quality, is_format_available, visible_accelerations, Acceleration, FormatGroup, OutputFormat, PlatformOs};
use crate::features::render::request::{detected_codecs, render_settings_draft, render_transport, RenderTransport};
use crate::features::render::session::{cancel_render, is_renderer_busy, RenderTarget};
use crate::fps::update_rate_options;
use crate::i18n::catalog;
use crate::store::Store;
use crate::store::agent::{AbsolutePath, AgentReply};
use crate::store::services::Services;

/// A render's settings; unset ones take the dialog's defaults (the committed settings, then the
/// dialog's rules).
#[derive(Clone, Debug, Default, serde::Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentRenderOptions {
    /// The output format: prores or qtrle (transparent), h264 or hevc (full video).
    pub format: Option<OutputFormat>,
    /// The encoder's acceleration; render_options lists the available ones per format.
    pub acceleration: Option<Acceleration>,
    /// transparent (the overlay alone) or composite (over the videos; needs videos).
    pub export_mode: Option<ExportMode>,
    /// combined (one file) or separate (one file per video; needs more than one video).
    pub output_mode: Option<OutputMode>,
    /// The frame rate; locked to the videos' rate for a full video export.
    pub fps: Option<NonZeroU32>,
    /// The widget update rate divisor, one of render_options' widgetUpdateRates.
    pub widget_update_rate: Option<u32>,
    /// MP4 rate control: quality (a CRF) or bitrate (Mbps).
    pub quality_type: Option<QualityType>,
    /// The CRF (1–51) or the bitrate in Mbps.
    pub quality_value: Option<f64>,
    /// Render the whole external activity as one transparent file, whatever the videos cover.
    pub render_full_activity: Option<bool>,
    /// The output file, absolute; default: the suggested file in the remembered folder.
    pub output_path: Option<AbsolutePath>,
    /// The folder of separate files, absolute; default: the saved folder. It is saved.
    pub output_folder: Option<AbsolutePath>,
    /// Replace an existing output file. Without it, OVRLEY asks the user and the call waits.
    pub overwrite: Option<bool>,
}

/// A start waiting for the review (videos), then for its submission.
#[derive(Debug)]
pub struct PendingAgentStart {
    overwrite: bool,
    reply: AgentReply,
}

/// The agent requests the dialog holds: a start before its submission, the start's reply until
/// acceptance, and the progress waits.
#[derive(Debug, Default)]
pub struct AgentRequests {
    pub(super) start: Option<PendingAgentStart>,
    pub(super) reply: Option<AgentReply>,
    waiters: Vec<(u64, AgentReply)>,
    next_waiter: u64,
}

fn mode_name(mode: ExportMode) -> &'static str {
    match mode {
        ExportMode::Transparent => "transparent",
        ExportMode::Composite => "composite",
    }
}

/// What blocks a render now, `None` when nothing does.
fn blocker(store: &Store) -> Option<String> {
    if let Some(reason) = render_disabled_reason(store) {
        return Some(reason);
    }
    if store.render_dialog.is_open() {
        return Some(
            "The render dialog is open in the window; finish or close it there first.".into(),
        );
    }
    let missing = store
        .video_import
        .imported_videos
        .iter()
        .filter(|video| video.missing)
        .count();
    if missing > 0 {
        return Some(catalog().t_count(store.i18n.language, "render-video.videosMissingDescription", missing, "{{count}} videos could not be found. Locate them or remove them from the project before exporting."));
    }
    if resolution_mismatch(store) {
        return Some(view::render_dialog_view_mismatch_text(store));
    }
    None
}

/// `render_options`: what the dialog offers now.
pub fn render_options(store: &Store) -> Value {
    let draft = render_settings_draft(store, String::new());
    let platform = PlatformOs::current();
    let codecs = detected_codecs(store);
    let formats: Vec<Value> = OutputFormat::ALL
        .into_iter()
        .map(|format| {
            let accelerations: Vec<Value> =
                visible_accelerations(format, platform, codecs).into_iter().map(|option| json!({"acceleration": option.acceleration.value(), "label": option.acceleration.label(), "available": option.available})).collect();
            json!({
                "format": format.value(),
                "label": format.label(),
                "exportMode": if format.group() == FormatGroup::Mp4 { "composite" } else { "transparent" },
                "available": is_format_available(format, platform, codecs),
                "accelerations": accelerations,
                "defaultQuality": (format.group() == FormatGroup::Mp4).then(|| default_quality(format)),
            })
        })
        .collect();
    let has_videos = store.video_import.has_videos();
    json!({
        "codecDetection": if codecs.is_some() { "done" } else { "pending" },
        "defaults": {
            "exportMode": mode_name(draft.settings.export_mode),
            "outputMode": if draft.settings.output_mode == OutputMode::Separate { "separate" } else { "combined" },
            "codec": draft.settings.codec,
            "fps": draft.settings.fps,
            "widgetUpdateRate": draft.settings.widget_update_rate,
        },
        "formats": formats,
        "lockedFps": imported_video_fps(store).filter(|_| has_videos).map(f64::round),
        "widgetUpdateRates": update_rate_options(update_rate_fps(store, &draft)),
        "quality": {"min": QUALITY_SLIDER_MIN, "max": QUALITY_SLIDER_MAX},
        "bitrate": {"min": 5, "max": 100, "step": 5},
        "outputModes": store.video_import.imported_videos.len() > 1,
        "renderFullActivity": has_videos && store.media.parsed_activity.is_some(),
        "batchOutputFolder": store.batch_render.batch_output_folder,
        "blocker": blocker(store),
        "busy": is_renderer_busy(store),
    })
}

/// Applies the options to the open dialog's draft, through the dialog's own handlers.
fn apply_options(store: &mut Store, options: &AgentRenderOptions) -> Result<(), String> {
    let has_videos = store.video_import.has_videos();
    if let Some(mode) = options.export_mode {
        if mode == ExportMode::Composite && !has_videos {
            return Err("Full video export needs an imported video.".into());
        }
        set_export_mode(store, mode);
    }
    if let Some(mode) = options.output_mode {
        if store.video_import.imported_videos.len() < 2 && mode == OutputMode::Separate {
            return Err("Separate files need more than one video.".into());
        }
        set_output_mode(store, mode);
    }
    if let Some(fps) = options.fps {
        if store.render_dialog.draft.as_ref().is_some_and(|draft| {
            locked_video_fps(store, draft).is_some()
                && draft.settings.export_mode == ExportMode::Composite
        }) {
            return Err(
                "The frame rate is locked to the videos' rate for a full video export.".into(),
            );
        }
        set_custom_fps(store, f64::from(fps.get()));
    }
    if let Some(format) = options.format {
        if !is_format_available(format, PlatformOs::current(), detected_codecs(store)) {
            return Err(format!(
                "{} is not available on this machine.",
                format.label()
            ));
        }
        select_format(store, format);
        let selected = store
            .render_dialog
            .draft
            .as_ref()
            .and_then(|draft| codec_selection(&draft.settings.codec))
            .map(|(selected, _)| selected);
        if selected != Some(format) {
            return Err(format!(
                "{} cannot be used for this export.",
                format.label()
            ));
        }
    }
    if let Some(acceleration) = options.acceleration {
        let codec = store
            .render_dialog
            .draft
            .as_ref()
            .map(|draft| draft.settings.codec.clone())
            .unwrap_or_default();
        let Some((format, _)) = codec_selection(&codec) else {
            return Err("Choose a format before an acceleration.".into());
        };
        let available =
            visible_accelerations(format, PlatformOs::current(), detected_codecs(store))
                .iter()
                .any(|option| option.acceleration == acceleration && option.available);
        if !available {
            return Err(format!(
                "{} is not available for {}.",
                acceleration.label(),
                format.label()
            ));
        }
        select_acceleration(store, acceleration);
    }
    reconcile_draft(store);
    if let Some(rate) = options.widget_update_rate {
        let draft = store
            .render_dialog
            .draft
            .as_ref()
            .expect("the dialog is open");
        let rates = update_rate_options(update_rate_fps(store, draft));
        if !rates.contains(&rate) {
            return Err(format!("The widget update rate must be one of {rates:?}."));
        }
        set_update_rate(store, rate);
    }
    if let Some(quality_type) = options.quality_type {
        set_quality_type(store, quality_type);
    }
    if let Some(value) = options.quality_value {
        let draft = store
            .render_dialog
            .draft
            .as_mut()
            .expect("the dialog is open");
        let quality_type = match draft.settings.quality {
            EncodingQuality::Quality(_) => QualityType::Quality,
            EncodingQuality::Bitrate(_) => QualityType::Bitrate,
        };
        draft.settings.quality =
            EncodingQuality::try_new(quality_type, value).map_err(|error| error.to_string())?;
    }
    if let Some(enabled) = options.render_full_activity {
        if enabled
            && !store.render_dialog.draft.as_ref().is_some_and(|draft| {
                has_videos
                    && store.media.parsed_activity.is_some()
                    && draft.settings.export_mode == ExportMode::Transparent
                    && draft.settings.output_mode == OutputMode::Combined
            })
        {
            return Err("The whole activity renders only as one transparent file of an external activity with videos.".into());
        }
        set_render_full_activity(store, enabled);
    }
    if let Some(path) = &options.output_path {
        set_output_path(store, path.clone().into());
    }
    if let Some(folder) = &options.output_folder {
        set_batch_output_folder(store, Some(folder.clone().into()));
    }
    Ok(())
}

/// `render_start`: the dialog opens with the options, as a user's render would, and its Start
/// runs once the review allows it. Replies on acceptance.
pub fn agent_start(
    store: &mut Store,
    services: &mut Services,
    options: AgentRenderOptions,
    reply: AgentReply,
) {
    if let Some(reason) = blocker(store) {
        return reply.send(Err(reason));
    }
    if is_renderer_busy(store) {
        return reply.send(Err(text(
            store,
            "render-video.renderingAlreadyInProgress",
            "Rendering already in progress",
        )));
    }
    open_render_dialog(store, services);
    if store.render_dialog.phase != DialogPhase::Confirm {
        return reply.send(Err(store
            .media
            .error_message
            .clone()
            .unwrap_or_else(|| "The render dialog could not open.".into())));
    }
    if let Err(error) = apply_options(store, &options) {
        close_render_dialog(store);
        return reply.send(Err(error));
    }
    store.render_dialog.agent.start = Some(PendingAgentStart {
        overwrite: options.overwrite.unwrap_or(false),
        reply,
    });
}

fn fail_start(store: &mut Store, reply: AgentReply, message: String) {
    close_render_dialog(store);
    reply.send(Err(message));
}

/// The reconcile part of agent requests: a pending start submits once its review allows it, and
/// progress waits answer once the renderer is idle.
pub(super) fn reconcile_agent_requests(store: &mut Store, services: &mut Services) {
    if !store.render_dialog.agent.waiters.is_empty() && !is_renderer_busy(store) {
        let progress = describe_progress(store);
        for (_, reply) in std::mem::take(&mut store.render_dialog.agent.waiters) {
            reply.send(Ok(progress.clone()));
        }
    }
    let dialog = &store.render_dialog;
    if dialog.agent.start.is_none()
        || dialog.phase != DialogPhase::Confirm
        || store.media.render_submission_target.is_some()
    {
        return;
    }
    let Some(draft) = dialog.draft.clone() else {
        return;
    };
    let pending = store
        .render_dialog
        .agent
        .start
        .take()
        .expect("checked above");
    if start_blocked(store, &draft) {
        let reason = if codec_selection(&draft.settings.codec).is_none() {
            "The project's codec is not one OVRLEY offers; pass format.".to_string()
        } else if resolution_mismatch(store) {
            view::render_dialog_view_mismatch_text(store)
        } else {
            format!(
                "The codec {} cannot be used for this export.",
                draft.settings.codec
            )
        };
        return fail_start(store, pending.reply, reason);
    }
    if render_transport(store, Some(&draft)) == RenderTransport::Batch {
        if !store.render_dialog.inspection.complete() {
            store.render_dialog.agent.start = Some(pending);
            return;
        }
        let review = store.render_dialog.review().cloned().unwrap_or_default();
        if !review.blockers.is_empty() {
            return fail_start(store, pending.reply, review.blockers.join(" "));
        }
        if ready_batch(store).is_none() {
            let blocked: Vec<String> = review
                .rows
                .iter()
                .filter_map(|row| {
                    row.error
                        .as_ref()
                        .map(|error| format!("{}: {error}", row.filename))
                })
                .collect();
            let reason = if draft.settings.output_mode == OutputMode::Separate
                && store.batch_render.batch_output_folder.is_none()
            {
                "Choose an output folder: pass outputFolder.".to_string()
            } else if blocked.is_empty() {
                "The videos cannot be exported.".to_string()
            } else {
                blocked.join("; ")
            };
            return fail_start(store, pending.reply, reason);
        }
    }
    store.render_dialog.agent.reply = Some(pending.reply);
    start_render(store, services, pending.overwrite, None);
    if store.media.render_submission_target.is_none() {
        // The submission was refused before it started (an empty output path, a request error).
        if let Some(reply) = store.render_dialog.agent.reply.take() {
            let message = store
                .render_dialog
                .path_error
                .as_ref()
                .map(|error| format_render_path_error(store, error))
                .or_else(|| store.media.error_message.clone())
                .unwrap_or_else(|| "The render did not start.".into());
            close_render_dialog(store);
            reply.send(Err(message));
        }
    }
}

/// The acceptance or refusal of an agent's submission.
pub(super) fn answer_submission(store: &mut Store, accepted: Option<Value>, error: Option<String>) {
    let Some(reply) = store.render_dialog.agent.reply.take() else {
        return;
    };
    match (accepted, error) {
        (Some(value), _) => reply.send(Ok(value)),
        (None, Some(message)) => {
            close_render_dialog(store);
            reply.send(Err(message));
        }
        // The overwrite question waits for the user.
        (None, None) => store.render_dialog.agent.reply = Some(reply),
    }
}

/// The overwrite question was cancelled: the agent's start did not happen.
pub(super) fn answer_overwrite_cancelled(store: &mut Store) {
    if let Some(reply) = store.render_dialog.agent.reply.take() {
        reply.send(Ok(json!({"started": false})));
    }
}

/// The current or last render, as agents read it.
pub fn describe_progress(store: &Store) -> Value {
    let target = match store.media.render_submission_target {
        Some(RenderTarget::Preview) => "preview",
        Some(RenderTarget::Single) => "single",
        Some(RenderTarget::Batch) => "batch",
        None if store.media.rendering_video => "single",
        None if store.batch_render.batch_snapshot.is_some() => "batch",
        None => "single",
    };
    if target == "batch" {
        if let Some(snapshot) = &store.batch_render.batch_snapshot {
            let rows: Vec<Value> = snapshot
                .items
                .iter()
                .map(|item| {
                    let (outcome, detail) = match &item.outcome {
                        Some(BatchItemOutcome::Succeeded { output_path }) => ("succeeded", Some(output_path.clone())),
                        Some(BatchItemOutcome::Failed { message }) => ("failed", Some(message.clone())),
                        Some(BatchItemOutcome::Cancelled) => ("cancelled", None),
                        Some(BatchItemOutcome::Unstarted) => ("unstarted", None),
                        None => ("pending", None),
                    };
                    json!({"id": item.id, "phase": item.phase, "outcome": outcome, "detail": detail, "plannedFrames": item.planned_frames, "currentFrames": item.current_frames})
                })
                .collect();
            let percent = if snapshot.planned_frames > 0 {
                snapshot.processed_frames as f64 / snapshot.planned_frames as f64 * 100.0
            } else {
                0.0
            };
            return json!({
                "target": "batch",
                "batchId": snapshot.batch_id,
                "status": snapshot.phase,
                "busy": snapshot.renderer_busy,
                "finished": !snapshot.renderer_busy && matches!(snapshot.phase, BatchPhase::Completed | BatchPhase::CompletedWithErrors | BatchPhase::Failed | BatchPhase::Cancelled),
                "percent": percent,
                "frames": {"current": snapshot.processed_frames, "total": snapshot.planned_frames},
                "renderFps": snapshot.current_item_progress.as_ref().and_then(|progress| progress.rendering_fps),
                "secondsRemaining": snapshot.estimated_seconds_remaining,
                "videos": rows,
                "outputs": snapshot.outputs.iter().map(|output| output.output_path.clone()).collect::<Vec<_>>(),
            });
        }
    }
    let progress = &store.media.render_progress;
    let percent = if progress.total > 0 {
        f64::from(progress.current) / f64::from(progress.total) * 100.0
    } else {
        0.0
    };
    json!({
        "target": target,
        "renderId": (progress.render_id > 0).then_some(progress.render_id),
        "status": progress.status,
        "busy": is_renderer_busy(store),
        "finished": progress.status.is_terminal(),
        "percent": percent,
        "frames": {"current": progress.current, "total": progress.total},
        "renderFps": progress.rendering_fps,
        "secondsRemaining": progress.estimated_seconds_remaining,
        "outputPath": store.media.active_render_output_path,
        "filename": progress.filename,
        "error": (progress.status == ovrley_core::encode::progress::RenderStatus::Error).then(|| progress.message.clone()),
    })
}

/// `render_progress`: now, or once the render ends or `wait_seconds` pass, whichever is first.
pub fn agent_progress(
    store: &mut Store,
    services: &mut Services,
    wait_seconds: f64,
    reply: AgentReply,
) {
    if wait_seconds <= 0.0 || !is_renderer_busy(store) {
        return reply.send(Ok(describe_progress(store)));
    }
    let agent = &mut store.render_dialog.agent;
    let id = agent.next_waiter;
    agent.next_waiter += 1;
    agent.waiters.push((id, reply));
    let deadline = std::time::Duration::from_secs_f64(wait_seconds);
    services.workers.spawn("render-progress-wait", move || {
        std::thread::sleep(deadline);
        Box::new(move |store: &mut Store, _: &mut Services| {
            let waiters = &mut store.render_dialog.agent.waiters;
            if let Some(index) = waiters.iter().position(|(waiter, _)| *waiter == id) {
                let (_, reply) = waiters.remove(index);
                reply.send(Ok(describe_progress(store)));
            }
        })
    });
}

/// `render_cancel`: cancels the running render; `render_progress` shows when cleanup ends.
pub fn agent_cancel(store: &mut Store, services: &mut Services) -> Result<Value, String> {
    let target = if store.media.rendering_video {
        RenderTarget::Single
    } else if store
        .batch_render
        .batch_snapshot
        .as_ref()
        .is_some_and(|snapshot| snapshot.renderer_busy)
    {
        RenderTarget::Batch
    } else {
        return Err("No render is running.".into());
    };
    cancel_render(store, services, target);
    Ok(json!({"cancelling": true}))
}

/// `ovrley_summary`'s `render`.
pub fn describe_render(store: &Store) -> Value {
    let progress = describe_progress(store);
    json!({"busy": is_renderer_busy(store), "target": progress["target"], "percent": progress["percent"]})
}
