//! The render dialog's presentation (Phase 7 D4): `useRenderVideoDialogState`'s returned values,
//! `getRenderSummaryItems` and the progress and queue rows of `renderPresentation.js`. Strings a
//! view builds are translated here; `ovrley_ui` copies the view into the Slint global.

use super::codecs::{
    codec_selection, invert_quality_value, is_format_available, is_mp4_codec, quality_value_label,
    visible_accelerations, Acceleration, FormatGroup, OutputFormat, PlatformOs,
};
use super::request::{detected_codecs, RenderTransport};
use super::session::{is_renderer_busy, RenderTarget};
use super::{
    format_render_path_error, imported_video_fps, locked_video_fps, ready_batch, render_transport,
    resolution_mismatch, start_blocked, update_rate_fps, video_format, DialogPhase,
};
use crate::i18n::{catalog, interpolate};
use crate::lib::activity::resolve_activity_window;
use crate::lib::choice_list::ChoiceKind;
use crate::lib::format::{
    format_duration_summary, format_locale_number, format_minutes_seconds, format_render_fps,
};
use crate::lib::update_rate::{container_fps, update_rate_options};
use crate::store::slices::media::activity_duration_seconds;
use crate::store::Store;
use ovrley_core::debug::RenderStatus;
use ovrley_core::encode::quality::EncodingQuality;
use ovrley_core::encode::render_settings::{
    ExportMode, OutputMode, QUALITY_SLIDER_MAX, QUALITY_SLIDER_MIN,
};
use ovrley_core::render_jobs::contracts::{
    BatchItemOutcome, BatchItemPhase, BatchPhase, BatchSnapshot,
};

/// One entry of a select: an item, a group label or a separator.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SelectEntry {
    pub kind: Option<ChoiceKind>,
    pub value: String,
    pub label: String,
    pub disabled: bool,
    /// An item's muted trailing text, or a label's badge.
    pub trailing: String,
    pub help: String,
}

fn item(value: &str, label: &str, disabled: bool, trailing: String) -> SelectEntry {
    SelectEntry {
        kind: Some(ChoiceKind::Item),
        value: value.into(),
        label: label.into(),
        disabled,
        trailing,
        help: String::new(),
    }
}

/// The MP4 rate control's section.
#[derive(Clone, Debug, PartialEq)]
pub struct QualityView {
    pub bitrate: bool,
    pub value_label: String,
    pub slider_value: f64,
    pub minimum: f64,
    pub maximum: f64,
    pub step: f64,
}

/// `RenderExportSettings`' values.
#[derive(Clone, Debug, PartialEq)]
pub struct SettingsView {
    pub show_export_mode: bool,
    pub export_mode: ExportMode,
    pub show_output_modes: bool,
    pub output_mode: OutputMode,
    pub locked: bool,
    /// The locked frame rate's text; `None` shows the select.
    pub fps_locked_text: Option<String>,
    pub fps_mode: String,
    pub fps: String,
    pub update_rates: Vec<u32>,
    pub update_rate: u32,
    pub container_fps_text: Option<String>,
    pub formats: Vec<SelectEntry>,
    pub format: String,
    pub accelerations: Vec<SelectEntry>,
    pub acceleration: String,
    pub quality: Option<QualityView>,
    pub show_render_full_activity: bool,
    pub render_full_activity: bool,
    /// Separate files: the folder picker's text; `None` shows the output file field.
    pub folder_text: Option<String>,
    pub output_path: String,
    pub path_error: Option<String>,
    /// Without videos the settings carry their own Cancel and Start.
    pub show_footer: bool,
    pub cancel_disabled: bool,
    pub start_disabled: bool,
    pub browse_disabled: bool,
}

/// `RenderProgressPanel`'s values.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProgressView {
    pub finished: bool,
    pub title: String,
    pub subtitle: String,
    pub summary: Vec<String>,
    pub percent_text: String,
    pub frames_text: String,
    pub percent: f32,
    /// Render FPS and Est. Remaining show while not finalizing.
    pub show_stats: bool,
    pub fps: String,
    pub remaining: String,
    pub cancel_shown: bool,
    pub cancelling: bool,
}

/// One row of the video list beside a video export's progress.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct QueueRowView {
    pub filename: String,
    pub tooltip: String,
    pub done: bool,
    /// Done, Queued or the percent; empty without progress.
    pub status_text: String,
    pub show_progress: bool,
    pub progress: f32,
    pub loading: bool,
    /// The active row's fps and time remaining.
    pub stats: Option<(String, String)>,
    pub note: String,
    pub error: String,
}

/// `RenderVideoDialog`'s values.
#[derive(Clone, Debug, PartialEq)]
pub struct RenderDialogView {
    pub phase: DialogPhase,
    /// Escape and pressing outside close it.
    pub dismissable: bool,
    /// The video list opens beside the progress.
    pub wide: bool,
    pub has_videos: bool,
    /// The resolution mismatch screen's message; it replaces the settings.
    pub mismatch: Option<String>,
    pub settings: SettingsView,
    pub progress: ProgressView,
    pub show_batch_progress: bool,
    pub rows: Vec<QueueRowView>,
    pub blockers: Vec<String>,
    pub review_error: Option<String>,
    pub close_text: String,
    pub close_disabled: bool,
    pub show_start: bool,
    pub start_running: bool,
    pub start_disabled: bool,
}

/// What the render dialog's surroundings show: its questions and the missing-videos notice.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RenderNoticesView {
    pub overwrite_path: Option<String>,
    pub missing: Option<(String, String)>,
}

fn t(store: &Store, key: &str, default: &'static str) -> String {
    catalog()
        .t_or(store.i18n.language, key, default)
        .to_string()
}

fn t_with(store: &Store, key: &str, default: &'static str, values: &[(&str, &str)]) -> String {
    interpolate(catalog().t_or(store.i18n.language, key, default), values)
}

fn integer(value: u64) -> String {
    format_locale_number(value as f64, 0)
}

fn batch_finished(snapshot: Option<&BatchSnapshot>) -> bool {
    snapshot.is_some_and(|snapshot| {
        matches!(
            snapshot.phase,
            BatchPhase::Completed
                | BatchPhase::CompletedWithErrors
                | BatchPhase::Failed
                | BatchPhase::Cancelled
        )
    })
}

pub fn render_notices_view(store: &Store) -> RenderNoticesView {
    let dialog = &store.render_dialog;
    let count = dialog.missing_video_count;
    RenderNoticesView {
        overwrite_path: dialog.pending_overwrite.clone(),
        missing: (count > 0).then(|| {
            (
                t(store, "render-video.videosMissingTitle", "Videos missing"),
                catalog().t_count(
                    store.i18n.language,
                    "render-video.videosMissingDescription",
                    count,
                    "{{count}} videos could not be found. Locate them or remove them from the project before exporting.",
                ),
            )
        }),
    }
}

/// The progress panel's values from counts (`createRenderProgress`, `createBatchProgress`).
struct Counts {
    current: u64,
    total: u64,
    encoded: u64,
    preparing: bool,
    cancelling: bool,
    fps: Option<f64>,
    remaining: Option<f64>,
}

fn progress_view(
    store: &Store,
    counts: Counts,
    summary: Vec<String>,
    finished: Option<String>,
) -> ProgressView {
    let percent = if counts.total > 0 {
        counts.current as f64 / counts.total as f64 * 100.0
    } else {
        0.0
    };
    let finalizing = percent >= 100.0;
    let subtitle = if counts.preparing {
        t(store, "render-video.preparingBatch", "Preparing video...")
    } else if counts.cancelling {
        t(store, "render-video.cancelling", "Cancelling...")
    } else if finalizing {
        if counts.encoded > 0 && counts.total > 0 {
            t_with(
                store,
                "render-video.encodingValVal2Frames",
                "Encoding: {{val}} / {{val2}} frames",
                &[
                    ("val", &integer(counts.encoded)),
                    ("val2", &integer(counts.total)),
                ],
            )
        } else {
            t(
                store,
                "render-video.encodingOutputFile",
                "Encoding output file...",
            )
        }
    } else {
        t(store, "render-video.renderingFrames", "Rendering frames...")
    };
    let title = match &finished {
        Some(title) => title.clone(),
        None if finalizing => t(store, "render-video.finalizingVideo", "Finalizing Video"),
        None => t(store, "render-video.exportingOverlay", "Exporting Overlay"),
    };
    ProgressView {
        finished: finished.is_some(),
        title,
        subtitle,
        summary,
        percent_text: t_with(
            store,
            "render-video.percentComplete",
            "{{percent}}% Complete",
            &[("percent", &percent.round().to_string())],
        ),
        frames_text: t_with(
            store,
            "render-video.frameProgress",
            "{{current}} / {{total}} frames",
            &[
                ("current", &integer(counts.current)),
                ("total", &integer(counts.total)),
            ],
        ),
        percent: percent as f32,
        show_stats: finished.is_none() && !finalizing,
        fps: format_render_fps(counts.fps),
        remaining: format_minutes_seconds(counts.remaining),
        cancel_shown: false,
        cancelling: false,
    }
}

/// `getRenderSummaryItems`: the size, rate, update rate, format, acceleration and length.
fn summary_items(store: &Store, draft: &super::request::RenderSettingsDraft) -> Vec<String> {
    let settings = &draft.settings;
    let composite = settings.export_mode == ExportMode::Composite;
    let fps = match imported_video_fps(store) {
        Some(fps) if composite => fps.round(),
        _ => settings.fps,
    };
    let selection = codec_selection(&settings.codec);
    let codecs = detected_codecs(store);
    let acceleration = selection.and_then(|(format, acceleration)| {
        visible_accelerations(format, PlatformOs::current(), codecs)
            .into_iter()
            .find(|option| {
                option.acceleration == acceleration
                    && option.available
                    && acceleration != Acceleration::Cpu
            })
            .map(|option| option.acceleration.label().to_string())
    });
    let activity_duration = store
        .media
        .parsed_activity
        .as_ref()
        .map_or(0.0, |imported| {
            activity_duration_seconds(&imported.activity)
        });
    let bounds = store.player.timeline.timeline().committed.bounds;
    let duration = if draft.render_full_activity {
        Some(activity_duration)
    } else if let Some(bounds) = bounds {
        Some(bounds.end_second - bounds.start_second)
    } else if composite {
        None
    } else {
        let window = resolve_activity_window(
            activity_duration,
            if store.video_import.has_videos() {
                None
            } else {
                store.media.activity_trim
            },
        );
        Some(window.end_second - window.start_second)
    };
    let scene = &store.editor.config.scene;
    let size = match (
        scene.get("width").and_then(serde_json::Value::as_u64),
        scene.get("height").and_then(serde_json::Value::as_u64),
    ) {
        (Some(width), Some(height)) if width > 0 && height > 0 => Some(format!("{width}x{height}")),
        _ => None,
    };
    [
        size,
        Some(format!("{fps} fps")),
        Some(t_with(
            store,
            "render-video.update1val",
            "Update 1/{{val}}",
            &[("val", &settings.widget_update_rate.to_string())],
        )),
        Some(selection.map_or_else(
            || settings.codec.clone(),
            |(format, _)| format.label().to_string(),
        )),
        acceleration,
        duration.map(|seconds| format_duration_summary(seconds, store.i18n.language)),
    ]
    .into_iter()
    .flatten()
    .collect()
}

fn settings_view(
    store: &Store,
    draft: &super::request::RenderSettingsDraft,
    batch_running: bool,
) -> SettingsView {
    let settings = &draft.settings;
    let has_videos = store.video_import.has_videos();
    let separate = settings.output_mode == OutputMode::Separate;
    let composite = settings.export_mode == ExportMode::Composite;
    let platform = PlatformOs::current();
    let codecs = detected_codecs(store);
    let locked_fps = locked_video_fps(store, draft);
    let fps_locked = composite && (separate || locked_fps.is_some());
    let rate_fps = update_rate_fps(store, draft);
    let unavailable = t(store, "render-video.unavailable", "Unavailable");
    let mut formats = vec![
        SelectEntry {
            kind: Some(ChoiceKind::Label),
            label: t(
                store,
                "render-video.transparentCodecs",
                "Transparent Codecs",
            ),
            trailing: if has_videos && !separate {
                t(store, "render-video.videoImported", "Video imported")
            } else {
                String::new()
            },
            ..SelectEntry::default()
        },
        SelectEntry {
            kind: Some(ChoiceKind::Separator),
            ..SelectEntry::default()
        },
    ];
    formats.extend(
        OutputFormat::ALL
            .into_iter()
            .filter(|format| format.group() == FormatGroup::Transparent)
            .map(|format| item(format.value(), format.label(), composite, String::new())),
    );
    formats.push(SelectEntry {
        kind: Some(ChoiceKind::Label),
        label: t(store, "render-video.mp4Codecs", "MP4 Codecs"),
        trailing: if has_videos {
            String::new()
        } else {
            t(store, "render-video.videoRequired", "Video required")
        },
        ..SelectEntry::default()
    });
    formats.push(SelectEntry {
        kind: Some(ChoiceKind::Separator),
        ..SelectEntry::default()
    });
    formats.extend(
        OutputFormat::ALL
            .into_iter()
            .filter(|format| format.group() == FormatGroup::Mp4)
            .map(|format| {
                let available = is_format_available(format, platform, codecs);
                item(
                    format.value(),
                    format.label(),
                    !composite || !available,
                    if available {
                        String::new()
                    } else {
                        unavailable.clone()
                    },
                )
            }),
    );
    let selection = codec_selection(&settings.codec);
    let accelerations = selection.map_or_else(Vec::new, |(format, _)| {
        visible_accelerations(format, platform, codecs)
            .into_iter()
            .map(|option| SelectEntry {
                help: if option.acceleration == Acceleration::Vaapi {
                    t(store, "render-video.hardwareAccelerationVaapiHelp", "")
                } else {
                    String::new()
                },
                ..item(
                    option.acceleration.value(),
                    option.acceleration.label(),
                    !option.available,
                    if option.available {
                        String::new()
                    } else {
                        unavailable.clone()
                    },
                )
            })
            .collect()
    });
    let quality = is_mp4_codec(&settings.codec).then(|| match settings.quality {
        EncodingQuality::Quality(crf) => QualityView {
            bitrate: false,
            value_label: quality_value_label(&settings.codec, settings.quality),
            slider_value: invert_quality_value(f64::from(crf)),
            minimum: f64::from(QUALITY_SLIDER_MIN),
            maximum: f64::from(QUALITY_SLIDER_MAX),
            step: 1.0,
        },
        EncodingQuality::Bitrate(mbps) => QualityView {
            bitrate: true,
            value_label: quality_value_label(&settings.codec, settings.quality),
            slider_value: mbps,
            minimum: 5.0,
            maximum: 100.0,
            step: 5.0,
        },
    });
    let container = container_fps(rate_fps, f64::from(settings.widget_update_rate));
    let container_text = format!("{container:.2}");
    let container_text = container_text
        .strip_suffix(".00")
        .unwrap_or(&container_text)
        .to_string();
    let submission_pending = store.media.render_submission_target == Some(RenderTarget::Single);
    let path_error = store
        .render_dialog
        .path_error
        .as_ref()
        .map(|error| format_render_path_error(store, error));
    SettingsView {
        show_export_mode: has_videos,
        export_mode: settings.export_mode,
        show_output_modes: store.video_import.imported_videos.len() > 1,
        output_mode: settings.output_mode,
        locked: batch_running,
        fps_locked_text: fps_locked.then(|| {
            if separate {
                t(
                    store,
                    "render-video.lockedToEachVideoFps",
                    "Locked to each video's FPS",
                )
            } else {
                t_with(
                    store,
                    "render-video.lockedToVideoFps",
                    "Locked to video FPS ({{fps}} fps)",
                    &[("fps", &locked_fps.unwrap_or_default().round().to_string())],
                )
            }
        }),
        fps_mode: store.render_dialog.fps_mode.value(settings.fps),
        fps: settings.fps.to_string(),
        update_rates: update_rate_options(rate_fps),
        update_rate: settings.widget_update_rate,
        container_fps_text: (!separate || !composite).then(|| {
            t_with(
                store,
                "render-video.outputContainerFps",
                "Output container: {{fps}} fps",
                &[("fps", &container_text)],
            )
        }),
        formats,
        format: selection.map_or_else(String::new, |(format, _)| format.value().to_string()),
        accelerations,
        acceleration: selection.map_or_else(String::new, |(_, acceleration)| {
            acceleration.value().to_string()
        }),
        quality,
        show_render_full_activity: has_videos
            && store.media.parsed_activity.is_some()
            && !composite
            && !separate,
        render_full_activity: has_videos && draft.render_full_activity,
        folder_text: (has_videos && separate).then(|| {
            store
                .batch_render
                .batch_output_folder
                .clone()
                .unwrap_or_else(|| t(store, "render-video.chooseFolder", "Choose folder..."))
        }),
        output_path: draft.output_path.clone(),
        path_error,
        show_footer: !has_videos,
        cancel_disabled: store.media.rendering_video || submission_pending,
        start_disabled: start_blocked(store, draft)
            || submission_pending
            || draft.output_path.is_empty(),
        browse_disabled: submission_pending,
    }
}

/// `batchResultQueue` and `BatchRenderQueueRow`: the export's rows.
fn queue_rows(store: &Store, snapshot: &BatchSnapshot, batch_running: bool) -> Vec<QueueRowView> {
    snapshot
        .items
        .iter()
        .map(|item| {
            let filename = item
                .id
                .rsplit(['/', '\\'])
                .next()
                .unwrap_or_default()
                .to_string();
            let output_path = match &item.outcome {
                Some(BatchItemOutcome::Succeeded { output_path }) => Some(output_path.clone()),
                _ => None,
            };
            let active = snapshot.active_item_id.as_deref() == Some(item.id.as_str());
            let done = matches!(item.outcome, Some(BatchItemOutcome::Succeeded { .. }));
            let rendering =
                active && item.outcome.is_none() && item.phase == BatchItemPhase::Rendering;
            let queued =
                batch_running && item.outcome.is_none() && item.phase == BatchItemPhase::Queued;
            let progress = snapshot.current_item_progress.as_ref().filter(|_| active);
            let percent = progress.map_or(0.0, |progress| {
                if progress.planned_frames > 0 {
                    progress.current_frames as f64 / progress.planned_frames as f64 * 100.0
                } else {
                    0.0
                }
            });
            let has_progress = done || rendering || queued;
            QueueRowView {
                filename,
                tooltip: output_path.unwrap_or_else(|| item.id.clone()),
                done,
                status_text: if done {
                    t(store, "render-video.done", "Done")
                } else if queued {
                    t(store, "render-video.queued", "Queued")
                } else if has_progress {
                    format!("{}%", percent.round())
                } else {
                    String::new()
                },
                show_progress: has_progress,
                progress: if done {
                    100.0
                } else if queued {
                    0.0
                } else {
                    percent as f32
                },
                loading: item.outcome.is_none()
                    && matches!(
                        item.phase,
                        BatchItemPhase::Preparing | BatchItemPhase::Rendering
                    ),
                stats: rendering.then(|| {
                    (
                        format!(
                            "{}: {}",
                            t(store, "render-video.renderFps", "Render FPS"),
                            format_render_fps(progress.and_then(|progress| progress.rendering_fps))
                        ),
                        format!(
                            "{}: {}",
                            t(store, "render-video.estRemaining", "Est. Remaining"),
                            format_minutes_seconds(
                                progress.and_then(|progress| progress.estimated_seconds_remaining)
                            )
                        ),
                    )
                }),
                note: match (&item.outcome, item.phase) {
                    (Some(BatchItemOutcome::Cancelled), _) => {
                        t(store, "render-video.cancelled", "Cancelled")
                    }
                    (Some(BatchItemOutcome::Unstarted), _) => {
                        t(store, "render-video.unstarted", "Not started")
                    }
                    (None, BatchItemPhase::Preparing) => {
                        t(store, "render-video.preparingBatch", "Preparing video...")
                    }
                    _ => String::new(),
                },
                error: match &item.outcome {
                    Some(BatchItemOutcome::Failed { message }) => message.clone(),
                    _ => String::new(),
                },
            }
        })
        .collect()
}

/// The dialog's view, `None` while it is closed.
pub fn render_dialog_view(store: &Store) -> Option<RenderDialogView> {
    let dialog = &store.render_dialog;
    if dialog.phase == DialogPhase::Closed {
        return None;
    }
    let draft = dialog.draft.as_ref()?;
    let has_videos = store.video_import.has_videos();
    let snapshot = store.batch_render.batch_snapshot.as_ref();
    let batch_running = snapshot.is_some_and(|snapshot| snapshot.renderer_busy)
        || store.media.render_submission_target == Some(RenderTarget::Batch);
    let finished = batch_finished(snapshot);
    let show_batch_progress = has_videos && (batch_running || finished);
    let summary = summary_items(store, draft);
    let cancelling = store.media.render_cancellation_target.is_some()
        || snapshot.is_some_and(|snapshot| snapshot.phase == BatchPhase::Cancelling);
    let progress = if dialog.phase == DialogPhase::Progress {
        let progress = &store.media.render_progress;
        ProgressView {
            cancel_shown: true,
            cancelling: store.media.render_cancellation_target.is_some(),
            ..progress_view(
                store,
                Counts {
                    current: u64::from(progress.current),
                    total: u64::from(progress.total),
                    encoded: u64::from(progress.encoded),
                    preparing: progress.status == RenderStatus::Preparing,
                    cancelling: progress.status == RenderStatus::Cancelling,
                    fps: progress.rendering_fps,
                    remaining: progress
                        .estimated_seconds_remaining
                        .map(|seconds| seconds as f64),
                },
                summary,
                None,
            )
        }
    } else if let Some(snapshot) = snapshot {
        let finished_title = finished.then(|| match snapshot.phase {
            BatchPhase::CompletedWithErrors => t(
                store,
                "render-video.batchResult.completedWithErrors",
                "Batch finished with errors",
            ),
            BatchPhase::Failed => t(store, "render-video.batchResult.failed", "Batch failed"),
            BatchPhase::Cancelled => t(
                store,
                "render-video.batchResult.cancelled",
                "Batch cancelled",
            ),
            _ => t(store, "render-video.exportFinished", "Export Finished"),
        });
        progress_view(
            store,
            Counts {
                current: snapshot.processed_frames,
                total: snapshot.planned_frames,
                encoded: snapshot.encoded_frames,
                preparing: matches!(snapshot.phase, BatchPhase::Preparing | BatchPhase::Accepted),
                cancelling: snapshot.phase == BatchPhase::Cancelling,
                fps: snapshot
                    .current_item_progress
                    .as_ref()
                    .and_then(|progress| progress.rendering_fps),
                remaining: snapshot.estimated_seconds_remaining,
            },
            summary,
            finished_title,
        )
    } else {
        ProgressView {
            summary,
            ..ProgressView::default()
        }
    };
    let renders_parts = render_transport(store, Some(draft)) == RenderTransport::Batch;
    let blocked = start_blocked(store, draft);
    let submission_pending = store.media.render_submission_target == Some(RenderTarget::Single);
    let review = dialog.review();
    let scene = &store.editor.config.scene;
    let mismatch = resolution_mismatch(store).then(|| {
        let (resolution, _) = video_format(store).expect("a mismatch has a video format");
        let overlay = format!("{}x{}", scene.get("width").and_then(serde_json::Value::as_u64).unwrap_or(0), scene.get("height").and_then(serde_json::Value::as_u64).unwrap_or(0));
        t_with(
            store,
            "render-video.videoResolutionMismatchMessage",
            "Overlay resolution ({{overlayResolution}}) must match imported video ({{videoResolution}}).",
            &[("overlayResolution", &overlay), ("videoResolution", &format!("{}x{}", resolution.width, resolution.height))],
        )
    });
    Some(RenderDialogView {
        phase: dialog.phase,
        dismissable: dialog.phase != DialogPhase::Progress && !submission_pending && !batch_running,
        wide: store.video_import.imported_videos.len() > 1 && show_batch_progress,
        has_videos,
        mismatch,
        settings: settings_view(store, draft, batch_running),
        progress,
        show_batch_progress,
        rows: snapshot.map_or_else(Vec::new, |snapshot| {
            queue_rows(store, snapshot, batch_running)
        }),
        blockers: if show_batch_progress || draft.render_full_activity {
            Vec::new()
        } else {
            review.map_or_else(Vec::new, |review| review.blockers.clone())
        },
        review_error: (!show_batch_progress
            && !draft.render_full_activity
            && !dialog.inspection.stale.is_empty())
        .then(|| {
            t(
                store,
                "render-video.reinspectionRequired",
                "Sources changed or inspection expired. Inspect again before starting.",
            )
        }),
        close_text: if cancelling {
            t(store, "render-video.cancelling", "Cancelling...")
        } else if batch_running {
            t(store, "render-video.cancel", "Cancel")
        } else {
            t(store, "render-video.close", "Close")
        },
        close_disabled: store.media.render_submission_target == Some(RenderTarget::Batch)
            || cancelling,
        show_start: !finished,
        start_running: batch_running,
        start_disabled: if renders_parts {
            blocked || ready_batch(store).is_none()
        } else {
            blocked || submission_pending || draft.output_path.is_empty()
        } || is_renderer_busy(store),
    })
}
