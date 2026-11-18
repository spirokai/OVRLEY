//! The render request (Phase 7 D2): `renderConfig.js` and `renderRequest.js`.
//!
//! The render config is the editor-effective document (`editor_effective_config`) with the
//! render-only scene fields set on it, read once through core's render config ingress
//! (`parse_config_value`). `SceneConfig` requires the activity window, which the durable scene
//! leaves out, so the fields are set on the effective JSON before that one typed read. Core
//! validates at submission, as it does for the Tauri app; nothing here validates a render again.

use super::codecs::is_qsv_full_codec;
use crate::lib::activity::{resolve_activity_window, ActivityWindow};
use crate::lib::template::editor_effective_config;
use crate::lib::update_rate::normalize_update_rate_for_fps;
use crate::lib::video_program::program::resolve_preview_target;
use crate::lib::video_program::Clip;
use crate::store::slices::media::activity_duration_seconds;
use crate::store::Store;
use ovrley_core::activity::schema::ParsedActivity;
use ovrley_core::encode::ffmpeg::detect::AvailableCodecs;
use ovrley_core::encode::quality::{EncodingQuality, QualityType};
use ovrley_core::encode::render_settings::{ExportMode, OutputMode, RenderSettings};
use ovrley_core::normalize::template::template_manifest;
use ovrley_core::normalize::RenderConfig;
use ovrley_core::render_jobs::contracts::{
    BatchActivity, BatchDestination, BatchEncodingSettings, BatchRenderOutput,
    BatchRenderRequest,
};
use ovrley_widgets::model::document::OverlayConfig;
use ovrley_widgets::model::scene::ScenePresentationConfig;
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::sync::Arc;

/// The dialog's draft (`renderSettingsDraft`): the settings, the output file, and whether the
/// whole activity renders regardless of the videos.
#[derive(Clone, Debug, PartialEq)]
pub struct RenderSettingsDraft {
    pub settings: RenderSettings,
    pub output_path: String,
    pub render_full_activity: bool,
}

/// Which native service a render goes through.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderTransport {
    Single,
    Batch,
}

/// `renderTransport`: videos render through the batch service, unless the whole activity renders
/// as one overlay regardless of them.
pub fn render_transport(store: &Store, draft: Option<&RenderSettingsDraft>) -> RenderTransport {
    if store.video_import.has_videos() && !draft.is_some_and(|draft| draft.render_full_activity) {
        RenderTransport::Batch
    } else {
        RenderTransport::Single
    }
}

/// `createRenderSettingsDraft`: the committed settings, a single video always rendering to one
/// file, the fps truncated, the update rate normalized, transparent unless separate files or
/// videos, and the whole activity off. The output path comes from the caller.
pub fn render_settings_draft(store: &Store, output_path: String) -> RenderSettingsDraft {
    let committed = &store.render_settings;
    let video_count = store.video_import.imported_videos.len();
    let fps = committed.fps.trunc();
    RenderSettingsDraft {
        settings: RenderSettings {
            output_mode: if video_count > 1 {
                committed.output_mode
            } else {
                OutputMode::Combined
            },
            fps,
            widget_update_rate: normalize_update_rate_for_fps(
                fps,
                f64::from(committed.widget_update_rate),
            ),
            export_mode: if committed.output_mode == OutputMode::Separate || video_count > 0 {
                committed.export_mode
            } else {
                ExportMode::Transparent
            },
            codec: committed.codec.clone(),
            quality: committed.quality,
        },
        output_path,
        render_full_activity: false,
    }
}

/// What a single render or a preview frame renders: the clip, the activity driving its widgets,
/// the timeline second of the activity's zero, and the rendered window.
pub struct SingleRenderInputs<'a> {
    pub clip: Option<&'a Clip>,
    pub activity: Option<Arc<ParsedActivity>>,
    pub origin_second: f64,
    pub window: ActivityWindow,
}

/// `resolveSingleRenderInputs`: the activity overlay without videos, or the clip the preview
/// shows at the playhead. With an external activity the clip keeps its committed offset; without
/// one, the clip's own telemetry drives the overlay in clip-local time. The trim decides the
/// window only without videos.
pub fn single_render_inputs(store: &Store) -> SingleRenderInputs<'_> {
    let timeline = &store.player.timeline.timeline().committed;
    let clip = resolve_preview_target(&timeline.program, store.editor.selected_second).and_then(
        |target| {
            timeline
                .clips
                .iter()
                .find(|clip| clip.id() == target.video_id)
        },
    );
    let external = store
        .media
        .parsed_activity
        .as_ref()
        .map(|imported| Arc::clone(&imported.activity));
    match (external, clip) {
        (Some(activity), _) => {
            let trim = if store.video_import.has_videos() {
                None
            } else {
                store.media.activity_trim
            };
            let window = resolve_activity_window(activity_duration_seconds(&activity), trim);
            SingleRenderInputs {
                clip,
                activity: Some(activity),
                origin_second: 0.0,
                window,
            }
        }
        (None, None) => {
            let trim = if store.video_import.has_videos() {
                None
            } else {
                store.media.activity_trim
            };
            SingleRenderInputs {
                clip: None,
                activity: None,
                origin_second: 0.0,
                window: resolve_activity_window(0.0, trim),
            }
        }
        (None, Some(clip)) => {
            let activity = clip
                .video
                .telemetry
                .activity()
                .map(|imported| Arc::clone(&imported.activity));
            let duration = activity.as_deref().map_or(0.0, activity_duration_seconds);
            SingleRenderInputs {
                clip: Some(clip),
                activity,
                origin_second: clip.offset_seconds(),
                window: resolve_activity_window(duration, None),
            }
        }
    }
}

/// The detected codecs, once detection ended with a result (`availableCodecs`).
pub fn detected_codecs(store: &Store) -> Option<&AvailableCodecs> {
    match &store.video_import.available_codecs {
        crate::features::video_preview::filmstrips::CodecDetection::Done(codecs) => codecs.as_ref(),
        _ => None,
    }
}

fn effective_config(store: &Store) -> OverlayConfig {
    let theme = store
        .theme
        .as_ref()
        .expect("the theme colours are read before the first dispatch");
    editor_effective_config(
        &store.editor.config,
        &store.template.global_defaults,
        store.i18n.language,
        theme,
    )
}

/// `renderValues`: values without their `display_variants`, as React submits them.
fn render_values(values: &[Map<String, Value>]) -> Value {
    Value::Array(
        values
            .iter()
            .map(|value| {
                let mut value = value.clone();
                value.remove("display_variants");
                Value::Object(value)
            })
            .collect(),
    )
}

fn quality_fields(quality: EncodingQuality) -> (QualityType, f64) {
    match quality {
        EncodingQuality::Quality(crf) => (QualityType::Quality, f64::from(crf)),
        EncodingQuality::Bitrate(mbps) => (QualityType::Bitrate, mbps),
    }
}

fn quality_type_name(quality_type: QualityType) -> &'static str {
    match quality_type {
        QualityType::Quality => "quality",
        QualityType::Bitrate => "bitrate",
    }
}

/// `createRenderEffectiveConfig`: the effective document with the render-only scene fields: the
/// fps and update rate, the codec (with the QSV init arguments), the composite fields of the clip,
/// the activity window and the disabled laps. `video_offset_seconds` is the activity second where
/// the clip starts.
pub fn render_effective_config(
    store: &Store,
    settings: &RenderSettings,
    clip: Option<&Clip>,
    video_offset_seconds: f64,
    window: ActivityWindow,
    disabled_laps: &[usize],
) -> Result<RenderConfig, String> {
    let effective = effective_config(store);
    // A composite export without a clip, or a codec of the other mode, is rejected by core's
    // planning at submission; nothing here checks it again.
    let composite = settings.export_mode == ExportMode::Composite;
    let mut scene = effective.scene.clone();
    scene.insert("fps".into(), json!(settings.fps));
    scene.remove("updateRate");
    scene.insert("update_rate".into(), json!(settings.widget_update_rate));
    let mut ffmpeg = scene
        .get("ffmpeg")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    ffmpeg.insert("codec".into(), json!(settings.codec));
    if is_qsv_full_codec(&settings.codec) {
        // Without detected arguments core rejects the QSV profile at submission.
        let args = detected_codecs(store).map(|codecs| codecs.qsv_full_init_args.clone()).unwrap_or_default();
        ffmpeg.insert("qsv_full_init_args".into(), json!(args));
    } else {
        ffmpeg.remove("qsv_full_init_args");
    }
    scene.insert("ffmpeg".into(), Value::Object(ffmpeg));
    if let (true, Some(clip)) = (composite, clip) {
        let (quality_type, quality_value) = quality_fields(settings.quality);
        let video = &clip.video;
        for (key, value) in [
            ("width", json!(video.resolution.width)),
            ("height", json!(video.resolution.height)),
            ("composite_video_path", json!(video.path)),
            ("qualityType", json!(quality_type_name(quality_type))),
            ("qualityValue", json!(quality_value)),
            ("composite_sync_offset", json!(video_offset_seconds)),
            ("composite_video_fps_num", json!(video.fps.components().0)),
            ("composite_video_fps_den", json!(video.fps.components().1)),
            ("composite_video_duration", json!(video.duration_seconds)),
            ("composite_render_duration", json!(video.duration_seconds)),
            ("composite_video_trim_start", json!(0)),
            (
                "composite_widget_update_rate",
                json!(settings.widget_update_rate),
            ),
        ] {
            scene.insert(key.into(), value);
        }
    }
    scene.insert("start".into(), json!(window.start_second));
    scene.insert("end".into(), json!(window.end_second));
    scene.insert(
        "custom_export_range_active".into(),
        json!(composite || window.is_trimmed),
    );
    scene.insert("disabled_laps".into(), json!(disabled_laps));
    let document = json!({
        "scene": scene,
        "backdrops": effective.backdrops,
        "rasters": effective.rasters,
        "labels": effective.labels,
        "values": render_values(&effective.values),
        "plots": effective.plots,
    });
    ovrley_core::normalize::parse_config_value(&document).map_err(|error| error.to_string())
}

/// `createBatchRenderTemplate`: the shared presentation of a batch: the effective scene projected
/// on the manifest's presentation keys, and the materialized widgets. Each job owns its clock.
pub fn batch_render_template(
    store: &Store,
) -> Result<RenderConfig<ScenePresentationConfig>, String> {
    let effective = effective_config(store);
    let scene: Map<String, Value> = template_manifest()
        .scene_presentation_keys
        .iter()
        .filter_map(|key| {
            effective
                .scene
                .get(key)
                .map(|value| (key.clone(), value.clone()))
        })
        .collect();
    serde_json::from_value(json!({
        "scene": scene,
        "backdrops": effective.backdrops,
        "rasters": effective.rasters,
        "labels": effective.labels,
        "values": render_values(&effective.values),
        "plots": effective.plots,
    }))
    .map_err(|error| format!("batch template: {error}"))
}

/// `captureRenderEncoding`: the batch's encoding contract.
pub fn batch_encoding(
    settings: &RenderSettings,
    codecs: Option<&AvailableCodecs>,
) -> BatchEncodingSettings {
    let (quality_type, quality_value) = quality_fields(settings.quality);
    BatchEncodingSettings {
        export_mode: settings.export_mode,
        export_codec: settings.codec.clone(),
        // The draft's fps is whole (`createRenderSettingsDraft` truncates it).
        fps: settings.fps as u32,
        update_rate: settings.widget_update_rate,
        quality_type,
        quality_value,
        qsv_full_init_args: if is_qsv_full_codec(&settings.codec) {
            codecs.map(|codecs| codecs.qsv_full_init_args.clone())
        } else {
            None
        },
    }
}

/// `createExportDestination`: one combined file at the output path, or one file per video in the
/// output folder; `None` while the path or folder is missing.
pub fn export_destination(
    output_mode: OutputMode,
    output_path: &str,
    output_folder: Option<&str>,
    overwrite: bool,
) -> Option<BatchDestination> {
    match output_mode {
        OutputMode::Combined => (!output_path.is_empty()).then(|| BatchDestination::File {
            path: output_path.to_string(),
            overwrite,
        }),
        OutputMode::Separate => output_folder.map(|directory| BatchDestination::SourceNamed {
            directory: directory.to_string(),
        }),
    }
}

/// A reviewed export, ready to submit: its inspection, outputs and the committed offsets of
/// their sources.
#[derive(Clone, Debug)]
pub struct ReviewedBatch {
    pub inspection_id: String,
    pub outputs: Vec<BatchRenderOutput>,
    pub offsets: HashMap<String, f64>,
}

/// `createBatchRenderRequest`: with an external activity each source renders at its committed
/// offset; without one each uses its embedded telemetry.
pub fn batch_render_request(
    store: &Store,
    settings: &RenderSettings,
    review: &ReviewedBatch,
    destination: BatchDestination,
) -> Result<BatchRenderRequest, String> {
    let activity = match &store.media.parsed_activity {
        None => BatchActivity::EmbeddedActivity {},
        Some(imported) => BatchActivity::ExternalActivity {
            activity: Arc::clone(&imported.activity),
            offsets: review.offsets.clone(),
            disabled_laps: store.media.disabled_laps.clone(),
        },
    };
    Ok(BatchRenderRequest {
        inspection_id: review.inspection_id.clone(),
        template: batch_render_template(store)?,
        encoding: batch_encoding(settings, detected_codecs(store)),
        activity,
        destination,
        outputs: review.outputs.clone(),
    })
}

/// One submission, built from the store and the draft.
#[derive(Debug)]
pub enum RenderRequest {
    Single {
        config: RenderConfig,
        activity: Arc<ParsedActivity>,
        output_path: String,
        overwrite: bool,
    },
    Batch(BatchRenderRequest),
}

impl RenderRequest {
    pub fn transport(&self) -> RenderTransport {
        match self {
            Self::Single { .. } => RenderTransport::Single,
            Self::Batch(_) => RenderTransport::Batch,
        }
    }
}

fn single_render_config(
    store: &Store,
    inputs: &SingleRenderInputs,
    settings: &RenderSettings,
) -> Result<RenderConfig, String> {
    // Embedded telemetry carries no editor lap selection.
    let disabled_laps: &[usize] = if store.media.parsed_activity.is_none() {
        &[]
    } else {
        &store.media.disabled_laps
    };
    let offset = inputs
        .clip
        .map_or(0.0, |clip| clip.offset_seconds() - inputs.origin_second);
    render_effective_config(
        store,
        settings,
        inputs.clip,
        offset,
        inputs.window,
        disabled_laps,
    )
}

/// `createRenderRequest`: videos render the reviewed outputs through the batch service (combined
/// or separate); otherwise a single render produces the activity overlay. Returns the request and
/// the settings accepted with it (the draft without its path and whole-activity switch).
pub fn render_request(
    store: &Store,
    draft: &RenderSettingsDraft,
    review: Option<&ReviewedBatch>,
    overwrite: bool,
) -> Result<(RenderRequest, RenderSettings), String> {
    // The settings were validated where they entered the store and the draft; the scene, the
    // output path and the encoding are validated by core at submission.
    let settings = &draft.settings;
    if render_transport(store, Some(draft)) == RenderTransport::Batch {
        let destination = export_destination(
            settings.output_mode,
            &draft.output_path,
            store.batch_render.batch_output_folder.as_deref(),
            overwrite,
        )
        .ok_or("Render output target is required")?;
        let review = review.ok_or("A video export needs its review")?;
        return Ok((
            RenderRequest::Batch(batch_render_request(store, settings, review, destination)?),
            settings.clone(),
        ));
    }
    // The whole activity renders without a clip and untrimmed, whatever the videos cover.
    let inputs = if draft.render_full_activity {
        let activity = store
            .media
            .parsed_activity
            .as_ref()
            .map(|imported| Arc::clone(&imported.activity));
        let duration = activity.as_deref().map_or(0.0, activity_duration_seconds);
        SingleRenderInputs {
            clip: None,
            activity,
            origin_second: 0.0,
            window: resolve_activity_window(duration, None),
        }
    } else {
        single_render_inputs(store)
    };
    let config = single_render_config(store, &inputs, settings)?;
    let activity = inputs.activity.ok_or("No parsed activity available")?;
    Ok((
        RenderRequest::Single {
            config,
            activity,
            output_path: draft.output_path.clone(),
            overwrite,
        },
        settings.clone(),
    ))
}

/// The debug PNG frame's inputs: the config, the activity, and the second in that activity.
pub struct PreviewRenderRequest {
    pub config: RenderConfig,
    pub activity: Arc<ParsedActivity>,
    pub second: f64,
}

/// `createPreviewRenderRequest`: the full export scene of the clip the preview shows (or the
/// activity alone); the native renderer narrows it to the frame at `second`.
pub fn preview_render_request(store: &Store) -> Result<PreviewRenderRequest, String> {
    let inputs = single_render_inputs(store);
    let activity = inputs
        .activity
        .clone()
        .ok_or("No parsed activity available")?;
    let has_video = store.video_import.has_videos();
    let settings = RenderSettings {
        export_mode: if has_video {
            ExportMode::Composite
        } else {
            ExportMode::Transparent
        },
        codec: if has_video { "libx264" } else { "prores_ks" }.into(),
        ..store.render_settings.clone()
    };
    let config = single_render_config(store, &inputs, &settings)?;
    Ok(PreviewRenderRequest {
        config,
        activity,
        second: store.editor.selected_second - inputs.origin_second,
    })
}

/// `normalizeRenderOutputPath`: the path with the container extension of the export mode.
pub fn normalize_render_output_path(output_path: &str, export_mode: ExportMode) -> String {
    let extension = ovrley_core::output::RenderOutputKind::from(export_mode).extension();
    let split = output_path.rfind(['/', '\\']).map_or(0, |index| index + 1);
    let (directory, filename) = output_path.split_at(split);
    let stem = filename.rfind('.').map_or(filename, |dot| &filename[..dot]);
    format!("{directory}{stem}.{extension}")
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::lib::template::ThemeColors;
    use crate::lib::video_program::fixtures::{edits_at, video, TestVideo};
    use crate::lib::video_program::{ImportedVideo, VideoTelemetry};
    use crate::store::slices::media::ImportedActivity;
    use ovrley_core::activity::trim::ActivityTrim;
    use std::rc::Rc;

    pub(crate) fn imported(samples: &[f64], end: f64) -> ImportedActivity {
        ImportedActivity {
            activity: Arc::new(ParsedActivity {
                sample_elapsed_seconds: samples.to_vec(),
                trim_end_seconds: end,
                ..Default::default()
            }),
            preview_bytes: Arc::new(Vec::new()),
        }
    }

    /// A store with a template, the theme and a 20 s activity, as `renderVideo.test.js` sets up.
    pub(crate) fn store() -> Store {
        let mut store = Store::default();
        store.theme = Some(ThemeColors {
            ice: "#010101".into(),
            aqua: "#020202".into(),
            teal: "#030303".into(),
            accent: "#040404".into(),
        });
        store.editor.config = Rc::new(
            OverlayConfig::from_value(
                &json!({"scene": {"width": 1920, "height": 1080, "fps": 30}}),
            )
            .unwrap(),
        );
        store
            .media
            .activate_activity_file(imported(&[0.0, 10.0, 20.0], 20.0));
        store
    }

    /// `videoStoreState`: the videos at their offsets, with the timeline rebuilt.
    pub(crate) fn with_videos(
        store: &mut Store,
        videos: Vec<ImportedVideo>,
        offsets: &[(&str, f64)],
    ) {
        store.video_import.video_edits = Rc::new(edits_at(&videos, offsets));
        store.video_import.imported_videos = Rc::new(videos);
        crate::features::player::timeline::reconcile_timeline(store);
    }

    fn draft(store: &Store, update: impl FnOnce(&mut RenderSettings)) -> RenderSettingsDraft {
        let mut settings = store.render_settings.clone();
        update(&mut settings);
        RenderSettingsDraft {
            settings,
            output_path: "C:/renders/overlay.mov".into(),
            render_full_activity: false,
        }
    }

    fn transparent(settings: &mut RenderSettings) {
        settings.export_mode = ExportMode::Transparent;
    }

    fn single(request: RenderRequest) -> (RenderConfig, Arc<ParsedActivity>, String, bool) {
        match request {
            RenderRequest::Single {
                config,
                activity,
                output_path,
                overwrite,
            } => (config, activity, output_path, overwrite),
            RenderRequest::Batch(_) => panic!("a single render"),
        }
    }

    fn window(start_second: f64, end_second: f64, is_trimmed: bool) -> ActivityWindow {
        ActivityWindow {
            start_second,
            end_second,
            is_trimmed,
        }
    }

    fn effective(
        store: &Store,
        update: impl FnOnce(&mut RenderSettings),
        clip: Option<&Clip>,
        offset: f64,
        window: ActivityWindow,
    ) -> Result<RenderConfig, String> {
        let mut settings = store.render_settings.clone();
        update(&mut settings);
        render_effective_config(store, &settings, clip, offset, window, &[])
    }

    fn clip_of(test: TestVideo) -> Clip {
        Clip {
            video: video(test),
            edits: Default::default(),
        }
    }

    // renderConfig.test.js

    #[test]
    fn renders_the_trimmed_window_and_scopes_the_route() {
        let mut store = store();
        store.editor.config = Rc::new(
            OverlayConfig::from_value(&json!({"scene": {"width": 1920, "height": 1080, "fps": 60},
                "values": [{"id": "value-1", "value": "speed", "display_type": "text", "x": 10, "y": 20, "content_alignment": "right"}]}))
            .unwrap(),
        );
        store.template.global_defaults = Rc::new(
            json!({"color_values": "#ffffff"})
                .as_object()
                .unwrap()
                .clone(),
        );
        let config = effective(
            &store,
            |settings| {
                transparent(settings);
                settings.fps = 60.0;
                settings.widget_update_rate = 6;
            },
            None,
            0.0,
            window(5.25, 15.75, true),
        )
        .unwrap();
        assert_eq!(config.scene.update_rate, Some(6));
        assert_eq!(config.scene.ffmpeg["codec"], "prores_ks");
        assert_eq!((config.scene.start, config.scene.end), (5.25, 15.75));
        assert_eq!(config.scene.custom_export_range_active, Some(true));
        let value = serde_json::to_value(&config.values[0]).unwrap();
        assert_eq!(value["color"], "#ffffff");
        assert_eq!(value["content_alignment"], "right");
    }

    #[test]
    fn renders_the_whole_untrimmed_activity_without_scoping_the_route() {
        let store = store();
        let config = effective(&store, transparent, None, 0.0, window(0.0, 73.4, false)).unwrap();
        assert_eq!(
            (
                config.scene.start,
                config.scene.end,
                config.scene.custom_export_range_active
            ),
            (0.0, 73.4, Some(false))
        );
    }

    #[test]
    fn values_lose_their_display_variants_in_both_configs() {
        let mut store = store();
        store.editor.config = Rc::new(
            OverlayConfig::from_value(&json!({"scene": {"width": 1920, "height": 1080, "fps": 60, "start": 0, "end": 50},
                "values": [{"id": "lean-angle-1", "value": "lean_angle", "x": 10, "y": 20, "font": "Arial.ttf", "font_size": 60,
                    "display_type": "lean_angle", "display_variants": {"lean_angle": {"diameter": 180, "track_thickness": 24}}}]}))
            .unwrap(),
        );
        let captured = store.editor.config.clone();
        let config = effective(&store, transparent, None, 0.0, window(0.0, 50.0, false)).unwrap();
        let value = serde_json::to_value(&config.values[0]).unwrap();
        assert_eq!(
            (value["diameter"].as_f64(), value["track_thickness"].as_f64()),
            (Some(180.0), Some(24.0))
        );
        assert!(value.get("display_variants").is_none_or(Value::is_null));
        assert!(
            value.get("width").is_none_or(Value::is_null)
                && value.get("height").is_none_or(Value::is_null)
        );
        let template = batch_render_template(&store).unwrap();
        let value = serde_json::to_value(&template.values[0]).unwrap();
        assert_eq!(value["diameter"].as_f64(), Some(180.0));
        assert!(value.get("display_variants").is_none_or(Value::is_null));
        let scene = serde_json::to_value(&template.scene).unwrap();
        for key in ["start", "end", "fps"] {
            assert!(scene.get(key).is_none(), "{key} is not presentation");
        }
        assert_eq!(store.editor.config, captured, "the document is not changed");
    }

    #[test]
    fn takes_the_window_from_the_activity_when_the_template_omits_it() {
        let store = store();
        let clip = clip_of(TestVideo {
            path: "C:\\clip.mp4",
            duration_seconds: 24.0,
            ..TestVideo::default()
        });
        let config = effective(
            &store,
            |settings| {
                settings.export_mode = ExportMode::Composite;
                settings.codec = "libx264".into();
            },
            Some(&clip),
            0.0,
            window(3.0, 21.0, false),
        )
        .unwrap();
        assert_eq!(
            (
                config.scene.start,
                config.scene.end,
                config.scene.custom_export_range_active
            ),
            (3.0, 21.0, Some(true))
        );
    }

    #[test]
    fn a_transparent_override_with_a_video_has_no_composite_fields() {
        let store = store();
        let clip = clip_of(TestVideo {
            path: "C:\\clip.mp4",
            duration_seconds: 24.0,
            ..TestVideo::default()
        });
        let config = effective(
            &store,
            transparent,
            Some(&clip),
            0.0,
            window(5.25, 15.75, true),
        )
        .unwrap();
        assert_eq!(config.scene.ffmpeg["codec"], "prores_ks");
        assert_eq!(config.scene.custom_export_range_active, Some(true));
        assert_eq!(
            (
                config.scene.composite_video_path.clone(),
                config.scene.composite_render_duration
            ),
            (None, None)
        );
    }

    #[test]
    fn composite_fields_come_from_the_video_and_keep_a_negative_offset() {
        let store = store();
        let clip = clip_of(TestVideo {
            path: "C:\\portrait.mp4",
            duration_seconds: 20.0,
            fps: (60000, 2002),
            width: 1080,
            height: 1920,
            ..TestVideo::default()
        });
        let config = effective(
            &store,
            |settings| {
                settings.export_mode = ExportMode::Composite;
                settings.codec = "libx264".into();
                settings.fps = 60.0;
                settings.widget_update_rate = 2;
                settings.quality = EncodingQuality::Quality(18);
            },
            Some(&clip),
            4.0,
            window(0.0, 40.0, false),
        )
        .unwrap();
        let scene = &config.scene;
        assert_eq!(
            (scene.presentation.width, scene.presentation.height),
            (Some(1080), Some(1920))
        );
        assert_eq!(
            scene.composite_video_path.as_deref(),
            Some("C:\\portrait.mp4")
        );
        assert_eq!(scene.composite_sync_offset, Some(4.0));
        assert_eq!(
            (scene.composite_video_fps_num, scene.composite_video_fps_den),
            (Some(30000), Some(1001))
        );
        assert_eq!(
            (
                scene.composite_video_duration,
                scene.composite_render_duration
            ),
            (
                Some(clip.video.duration_seconds),
                Some(clip.video.duration_seconds)
            )
        );
        assert_eq!(
            (
                scene.composite_video_trim_start,
                scene.composite_widget_update_rate
            ),
            (Some(0.0), Some(2))
        );
        assert_eq!(
            (scene.quality_type, scene.quality_value),
            (Some(QualityType::Quality), Some(18.0))
        );

        let negative = effective(
            &store,
            |settings| {
                settings.export_mode = ExportMode::Composite;
                settings.codec = "libx264".into();
            },
            Some(&clip),
            -10.0,
            window(0.0, 25.0, false),
        )
        .unwrap();
        assert_eq!(
            (
                negative.scene.composite_sync_offset,
                negative.scene.composite_video_trim_start
            ),
            (Some(-10.0), Some(0.0))
        );
    }

    /// The request does not repeat core's checks: core's planning rejects a composite export
    /// without a video and a codec of the other mode.
    #[test]
    fn core_rejects_a_composite_without_a_video_and_a_mismatched_codec() {
        let store = store();
        let plan = |config| ovrley_core::render_jobs::planning::plan_single_render(config, 20.0, None, None).map(|_| ());
        let composite = |settings: &mut RenderSettings| {
            settings.export_mode = ExportMode::Composite;
            settings.codec = "libx264".into();
        };
        assert!(plan(effective(&store, composite, None, 0.0, window(0.0, 20.0, false)).unwrap()).is_err());
        let mp4_overlay = |settings: &mut RenderSettings| settings.codec = "libx264".into();
        let config = effective(&store, |settings| {
            transparent(settings);
            mp4_overlay(settings);
        }, None, 0.0, window(0.0, 20.0, false))
        .unwrap();
        assert!(plan(config).is_err());
        assert!(plan(effective(&store, transparent, None, 0.0, window(0.0, 20.0, false)).unwrap()).is_ok());
    }

    // renderVideo.test.js

    #[test]
    fn a_preview_frame_sends_the_full_trimmed_scene_with_its_second() {
        let mut store = store();
        store.editor.selected_second = 3.5;
        store.media.activity_trim = Some(ActivityTrim {
            start_second: 2.0,
            end_second: 12.0,
        });
        let request = preview_render_request(&store).unwrap();
        assert_eq!(
            (
                request.config.scene.start,
                request.config.scene.end,
                request.config.scene.custom_export_range_active
            ),
            (2.0, 12.0, Some(true))
        );
        assert_eq!(request.second, 3.5);
        assert!(
            Arc::ptr_eq(
                &request.activity,
                &store.media.parsed_activity.as_ref().unwrap().activity
            ),
            "the activity is shared, not copied"
        );
    }

    #[test]
    fn a_single_render_of_the_trimmed_activity() {
        let mut store = store();
        store.media.activity_trim = Some(ActivityTrim {
            start_second: 5.25,
            end_second: 15.75,
        });
        store.editor.config = Rc::new(
            OverlayConfig::from_value(&json!({"scene": {"width": 1920, "height": 1080, "fps": 60},
                "values": [{"id": "value-1", "value": "speed", "display_type": "text", "x": 10, "y": 20}]}))
            .unwrap(),
        );
        store.template.global_defaults = Rc::new(
            json!({"color_values": "#abcdef"})
                .as_object()
                .unwrap()
                .clone(),
        );
        let draft = draft(&store, |settings| {
            transparent(settings);
            settings.fps = 60.0;
            settings.widget_update_rate = 6;
        });
        let (request, accepted) = render_request(&store, &draft, None, false).unwrap();
        let (config, activity, output_path, overwrite) = single(request);
        assert_eq!(
            (
                config.scene.start,
                config.scene.end,
                config.scene.fps,
                config.scene.update_rate
            ),
            (5.25, 15.75, 60.0, Some(6))
        );
        assert_eq!(config.scene.custom_export_range_active, Some(true));
        assert_eq!(
            config.scene.ffmpeg,
            json!({"codec": "prores_ks"}),
            "applyCodecDefaults is not ported (step 0)"
        );
        assert_eq!(
            serde_json::to_value(&config.values[0]).unwrap()["color"],
            "#abcdef"
        );
        assert_eq!(activity.sample_elapsed_seconds, [0.0, 10.0, 20.0]);
        assert_eq!(
            (output_path.as_str(), overwrite),
            ("C:/renders/overlay.mov", false)
        );
        assert_eq!(accepted, draft.settings);
    }

    #[test]
    fn a_single_render_of_the_whole_untrimmed_activity() {
        let store = store();
        let (request, _) =
            render_request(&store, &draft(&store, transparent), None, false).unwrap();
        let (config, ..) = single(request);
        assert_eq!(
            (
                config.scene.start,
                config.scene.end,
                config.scene.custom_export_range_active
            ),
            (0.0, 20.0, Some(false))
        );
    }

    #[test]
    fn videos_render_through_the_batch_service_at_their_destination() {
        let mut store = store();
        with_videos(
            &mut store,
            vec![video(TestVideo {
                id: "clip",
                path: "C:/clip.mp4",
                duration_seconds: 30.0,
                ..TestVideo::default()
            })],
            &[("clip", 10.0)],
        );
        let mut draft = draft(&store, |settings| {
            settings.export_mode = ExportMode::Composite;
            settings.codec = "libx264".into();
        });
        draft.output_path = "C:/renders/video.mp4".into();
        let review = ReviewedBatch {
            inspection_id: "inspection".into(),
            outputs: vec![BatchRenderOutput {
                id: "C:/renders/video.mp4".into(),
                pieces: vec![ovrley_core::render_jobs::contracts::BatchRenderPiece {
                    source_id: "source".into(),
                    start_frame: 0,
                    end_frame: 900,
                    skip_overlay: false,
                }],
                transitions: Vec::new(),
            }],
            offsets: [("source".to_string(), 10.0)].into(),
        };
        let batch = |store: &Store, draft: &RenderSettingsDraft, overwrite| match render_request(
            store,
            draft,
            Some(&review),
            overwrite,
        ) {
            Ok((RenderRequest::Batch(request), _)) => Ok(request),
            Ok(_) => panic!("a batch"),
            Err(error) => Err(error),
        };
        let request = batch(&store, &draft, false).unwrap();
        assert_eq!(request.inspection_id, "inspection");
        let BatchActivity::ExternalActivity {
            offsets, activity, ..
        } = &request.activity
        else {
            panic!("an external activity")
        };
        assert_eq!(offsets["source"], 10.0);
        assert!(Arc::ptr_eq(
            activity,
            &store.media.parsed_activity.as_ref().unwrap().activity
        ));
        assert!(
            matches!(&request.destination, BatchDestination::File { path, overwrite: false } if path == "C:/renders/video.mp4")
        );
        assert_eq!(request.outputs.len(), 1);
        assert!(matches!(
            batch(&store, &draft, true).unwrap().destination,
            BatchDestination::File {
                overwrite: true,
                ..
            }
        ));

        draft.settings.output_mode = OutputMode::Separate;
        assert_eq!(
            batch(&store, &draft, false).unwrap_err(),
            "Render output target is required"
        );
        store.batch_render.batch_output_folder = Some("C:/renders".into());
        assert!(
            matches!(batch(&store, &draft, false).unwrap().destination, BatchDestination::SourceNamed { directory } if directory == "C:/renders")
        );
    }

    #[test]
    fn the_whole_activity_renders_as_one_overlay_regardless_of_videos_and_trim() {
        let mut store = store();
        with_videos(
            &mut store,
            vec![video(TestVideo {
                id: "clip",
                path: "C:/clip.mp4",
                duration_seconds: 5.0,
                ..TestVideo::default()
            })],
            &[("clip", 10.0)],
        );
        store.media.activity_trim = Some(ActivityTrim {
            start_second: 10.0,
            end_second: 15.0,
        });
        let mut draft = draft(&store, transparent);
        draft.render_full_activity = true;
        assert_eq!(
            render_transport(&store, Some(&draft)),
            RenderTransport::Single
        );
        let (request, _) = render_request(&store, &draft, None, false).unwrap();
        let (config, ..) = single(request);
        assert_eq!(
            (
                config.scene.start,
                config.scene.end,
                config.scene.custom_export_range_active
            ),
            (0.0, 20.0, Some(false))
        );
        assert_eq!(config.scene.composite_video_path, None);
    }

    #[test]
    fn the_trim_decides_only_without_videos() {
        let mut store = store();
        store.media.activity_trim = Some(ActivityTrim {
            start_second: 5.0,
            end_second: 8.0,
        });
        store.editor.selected_second = 10.0;
        let request = preview_render_request(&store).unwrap();
        assert_eq!(
            (
                request.config.scene.start,
                request.config.scene.end,
                request.second
            ),
            (5.0, 8.0, 10.0)
        );

        with_videos(
            &mut store,
            vec![video(TestVideo {
                id: "clip",
                path: "C:/clip.mp4",
                duration_seconds: 2.0,
                ..TestVideo::default()
            })],
            &[("clip", 15.0)],
        );
        let request = preview_render_request(&store).unwrap();
        assert_eq!(
            (
                request.config.scene.start,
                request.config.scene.end,
                request.second
            ),
            (0.0, 20.0, 10.0)
        );
    }

    #[test]
    fn a_preview_frame_of_a_clip_uses_its_own_telemetry_in_clip_local_time() {
        let mut store = store();
        store.media.clear_activity_file(true);
        let telemetry = imported(&[0.0, 15.0, 30.0], 30.0);
        let mut clip = video(TestVideo {
            id: "clip",
            path: "C:/clip.mp4",
            duration_seconds: 30.0,
            ..TestVideo::default()
        });
        clip.telemetry = VideoTelemetry::ready(telemetry.clone());
        let other = video(TestVideo {
            id: "other",
            path: "C:/other.mp4",
            duration_seconds: 30.0,
            import_order: 1,
            ..TestVideo::default()
        });
        with_videos(
            &mut store,
            vec![other, clip],
            &[("other", 0.0), ("clip", 100.0)],
        );
        store.editor.selected_second = 112.0;
        let request = preview_render_request(&store).unwrap();
        assert!(Arc::ptr_eq(&request.activity, &telemetry.activity));
        assert_eq!(request.second, 12.0);
        assert_eq!(
            request.config.scene.composite_video_path.as_deref(),
            Some("C:/clip.mp4")
        );
    }

    #[test]
    fn the_draft_follows_the_committed_settings() {
        let mut store = store();
        store.render_settings.fps = 29.97;
        store.render_settings.widget_update_rate = 7;
        store.render_settings.output_mode = OutputMode::Separate;
        let draft = render_settings_draft(&store, "/out.mov".into());
        assert_eq!(
            (draft.settings.fps, draft.settings.widget_update_rate),
            (29.0, 1),
            "the fps is truncated, the update rate normalized"
        );
        assert_eq!(
            draft.settings.output_mode,
            OutputMode::Combined,
            "one video or none renders to one file"
        );
        assert_eq!(
            draft.settings.export_mode,
            ExportMode::Composite,
            "separate files keep the export mode"
        );
        store.render_settings.output_mode = OutputMode::Combined;
        assert_eq!(
            render_settings_draft(&store, String::new())
                .settings
                .export_mode,
            ExportMode::Transparent,
            "no videos: transparent"
        );
    }

    // renderOutput.test.js
    #[test]
    fn output_paths_take_the_mode_extension_and_keep_the_stem() {
        assert_eq!(
            normalize_render_output_path("C:\\renders\\ride.mov", ExportMode::Transparent),
            "C:\\renders\\ride.mov"
        );
        assert_eq!(
            normalize_render_output_path("C:\\renders\\ride.mp4", ExportMode::Transparent),
            "C:\\renders\\ride.mov"
        );
        assert_eq!(
            normalize_render_output_path("/renders/ride.mov", ExportMode::Composite),
            "/renders/ride.mp4"
        );
        assert_eq!(
            normalize_render_output_path("/renders/ride", ExportMode::Composite),
            "/renders/ride.mp4"
        );
    }

    /// Step 0, check 2: `applyCodecDefaults` changes nothing core reads, so the FFmpeg arguments
    /// are equal with and without its fields.
    #[test]
    fn codec_defaults_change_no_ffmpeg_argument() {
        use ovrley_core::encode::ffmpeg::catalog::CodecSelection;
        use ovrley_core::normalize::validate_ffmpeg_config;
        for (codec, defaults) in [
            (
                "prores_ks",
                json!({"prores_profile": "4444", "pix_fmt": "yuva444p10le"}),
            ),
            (
                "prores_ks_vulkan",
                json!({"prores_profile": "4", "alpha_bits": 16}),
            ),
            ("qtrle", json!({"pix_fmt": "argb"})),
        ] {
            let selection = CodecSelection::from_external_name(codec).unwrap();
            let plain = validate_ffmpeg_config(json!({"codec": codec}), selection).unwrap();
            let mut with = defaults.as_object().unwrap().clone();
            with.insert("codec".into(), json!(codec));
            let with = validate_ffmpeg_config(Value::Object(with), selection).unwrap();
            let args = |config| {
                ovrley_core::encode::ffmpeg::settings::build_ffmpeg_settings(config)
                    .unwrap()
                    .output_args
            };
            assert_eq!(args(&plain), args(&with), "{codec}");
        }
    }
}
