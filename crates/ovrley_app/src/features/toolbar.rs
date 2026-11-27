//! The toolbar and its left drawer: the drawer's layout state and persisted preference, its
//! Escape rule, the activity drawer's view and the video drawer's rows and commands.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use ovrley_core::media::edits::TimezoneMode;
use ovrley_widgets::model::document::round_half_up;
use ovrley_widgets::widgets::lap_timer::format_lap_duration;
use serde::{Deserialize, Serialize};
use crate::dialogs::{filename_from_selected_path, open_single_path, DialogOptions};
use crate::features::import::{probe_video, queue_telemetry_extraction, LAST_VIDEO_IMPORT_DIR_KEY, VIDEO_EXTENSIONS};
use crate::features::transitions::{join_models, JoinModel};
use crate::features::widgets::activity_attribute_label;
use crate::format::{compare_text, format_clock_duration, format_locale_number, format_video_creation_time, format_zoned_date_time};
use crate::i18n::{catalog, interpolate, Locale};
use crate::preferences::PreferencesStore;
use crate::shortcuts::{match_keyboard_shortcut, KeyInput, Scope};
use crate::features::player::video_sync_mode;
use crate::store::Store;
use crate::store::media::{ActivitySource, ActivitySummary, MetricSource};
use crate::store::services::{DialogFilter, Services};
use crate::store::videos::{select_clip, select_clip_revealing_playhead, ClipLane, SelectedClip};
use crate::video::{Clip, TelemetryStatus};
use crate::video::collection::{format_video_warning, sort_clips_by_timeline};
use crate::video::edits::CreationTimeSource;
use crate::video::program::Program;
use crate::video::segments::{list_video_segments, segment_join_key};
use crate::video::sync::parse_video_filename_creation_time;

/// The drawer preference key.
pub const DRAWER_PREFERENCE_KEY: &str = "leftDrawer";

/// The drawer preference last read or written.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DrawerPreferenceState {
    pub last_observed: Option<DrawerPreference>,
}

/// The drawer preference in `value`. A value that is not an object with a boolean `pinned` and a
/// known `activeTool` becomes the default `{pinned: false, activeTool: "widgets"}`: a bad
/// preference never stops the app from starting.
pub fn normalize_drawer_preference(value: Option<&serde_json::Value>) -> DrawerPreference {
    value
        .and_then(|value| serde_json::from_value(value.clone()).ok())
        .unwrap_or_default()
}

/// Reads `leftDrawer` once and initializes the drawer from it. Runs before the window exists, so
/// the drawer state is known before the first frame.
pub fn hydrate_drawer_preference(store: &mut Store, preferences: &PreferencesStore) {
    let preference = normalize_drawer_preference(preferences.get(DRAWER_PREFERENCE_KEY));
    store.drawer_preference.last_observed = Some(preference);
    store.layout.initialize_left_drawer(preference);
}

/// Reconcile step: once the drawer is initialized, writes `{pinned, activeTool}` when it differs
/// from the last value seen.
pub fn persist_drawer_preference(store: &mut Store, services: &mut Services) {
    if !store.layout.left_drawer_initialized {
        return;
    }
    let preference = DrawerPreference {
        pinned: store.layout.left_drawer_pinned,
        active_tool: store.layout.active_left_drawer_tool,
    };
    if store.drawer_preference.last_observed == Some(preference) {
        return;
    }
    store.drawer_preference.last_observed = Some(preference);
    services.preferences.set(
        DRAWER_PREFERENCE_KEY,
        serde_json::to_value(preference).expect("a drawer preference serializes"),
    );
}

/// The capture-phase Escape rule (`drawer.close`): Escape dismisses an unpinned drawer. Returns
/// whether the key is consumed. A pinned drawer leaves Escape alone; an open select or popover
/// takes its own Escape before the window sees it.
pub fn handle_drawer_capture_key(store: &mut Store, input: &KeyInput) -> bool {
    let is_close = match_keyboard_shortcut(input, Scope::Drawer).is_some_and(|found| found.command.id == "drawer.close");
    if !is_close || !store.layout.is_unpinned_overlay() {
        return false;
    }
    store.layout.dismiss_left_drawer_overlay();
    true
}

/// One metadata row of the activity drawer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActivityRow {
    pub label: String,
    pub value: String,
}

/// One lap row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LapRow {
    pub id: usize,
    pub number: String,
    pub duration: String,
    pub start: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActivityDrawerView {
    pub file_name: String,
    pub can_delete: bool,
    pub format_label: String,
    pub metadata: Vec<ActivityRow>,
    pub extracted: Vec<String>,
    pub derived: Vec<String>,
    pub laps: Vec<LapRow>,
}

/// The duration as the lap timer widget shows it and the start
/// as a clock duration.
pub fn lap_rows(summary: &ActivitySummary, disabled: &[usize]) -> Vec<LapRow> {
    summary
        .laps
        .iter()
        .map(|lap| LapRow {
            id: lap.id,
            number: (lap.id + 1).to_string(),
            duration: format_lap_duration(lap.duration_seconds),
            start: format_clock_duration(lap.start_seconds),
            enabled: !disabled.contains(&lap.id),
        })
        .collect()
}

/// The activity drawer's view of the loaded activity; `None` without one.
pub fn activity_drawer_view(store: &Store) -> Option<ActivityDrawerView> {
    let summary = store.media.activity_summary.as_ref()?;
    let locale = store.i18n.language;
    let t = |key: &str| catalog().t(locale, key).to_string();
    let unknown = t("toolbar.unknown");
    let start_time = match &summary.timezone {
        Some(timezone) => summary
            .sync_time
            .as_deref()
            .and_then(|sync_time| format_zoned_date_time(sync_time, timezone, locale))
            .unwrap_or_else(|| unknown.clone()),
        None => unknown.clone(),
    };
    let meters = summary.total_distance_meters;
    let distance = if meters >= 1000.0 {
        format!("{} km", format_locale_number(meters / 1000.0, 2))
    } else {
        format!("{} m", format_locale_number(round_half_up(meters), 3))
    };
    let mut metadata = vec![
        ActivityRow { label: t("toolbar.startTime"), value: start_time },
        ActivityRow { label: t("toolbar.timezone"), value: summary.timezone.clone().unwrap_or_else(|| unknown.clone()) },
        ActivityRow { label: t("toolbar.distance"), value: distance },
        ActivityRow { label: t("toolbar.duration"), value: format_clock_duration(summary.duration_seconds) },
        ActivityRow { label: t("toolbar.activityType"), value: summary.sport.clone().unwrap_or_else(|| unknown.clone()) },
    ];
    if let Some(count) = summary.original_sample_count {
        metadata.push(ActivityRow {
            label: t("toolbar.extracted"),
            value: catalog().t_with(locale, "toolbar.valPoints", &[("val", &format_locale_number(count, 3))]),
        });
    }
    let mut extracted = Vec::new();
    let mut derived = Vec::new();
    for metric in &summary.available_metrics {
        let label = activity_attribute_label(&metric.attribute, locale);
        match metric.source {
            MetricSource::Direct => extracted.push(label),
            MetricSource::Derived | MetricSource::Mixed => derived.push(label),
        }
    }
    extracted.sort_by(|left, right| compare_text(left, right));
    derived.sort_by(|left, right| compare_text(left, right));
    let file_name = match &store.media.activity_source {
        Some(ActivitySource::File { path }) => filename_from_selected_path(path).unwrap_or(path).to_string(),
        None => summary.file_name.clone().unwrap_or_default(),
    };
    Some(ActivityDrawerView {
        file_name,
        // Only an external activity can be removed.
        can_delete: store.media.parsed_activity.is_some(),
        format_label: summary.file_format.as_deref().map_or(unknown, str::to_uppercase),
        metadata,
        extracted,
        derived,
        laps: lap_rows(summary, &store.media.disabled_laps),
    })
}

fn t(locale: Locale, key: &str, default: &'static str) -> String {
    catalog().t_or(locale, key, default).to_string()
}

/// One row of the details list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetadataRow {
    pub label: String,
    pub value: String,
}

/// Duration, frame rate, resolution, created at, time source,
/// codec, bit rate, camera and telemetry.
pub fn metadata_rows(clip: &Clip, timezone: Option<&str>, locale: Locale) -> Vec<MetadataRow> {
    let video = &clip.video;
    let unknown = t(locale, "toolbar.unknown", "Unknown");
    let time_source = match video.time_source.as_deref() {
        None | Some("") => unknown.clone(),
        Some("gps") => t(locale, "toolbar.timeSourceGps", "GPS timestamp"),
        Some("ffprobe") => t(locale, "toolbar.timeSourceMetadata", "Video metadata"),
        Some("filename") => t(locale, "toolbar.timeSourceFilename", "Filename"),
        Some(other) => interpolate(
            &t(locale, "toolbar.unknownTimeSource", "Unknown ({{value}})"),
            &[("value", other)],
        ),
    };
    let rate = crate::format::plain_number((video.fps.as_f64() * 100.0).round() / 100.0);
    let bit_rate = video
        .bit_rate
        .as_deref()
        .filter(|rate| !rate.is_empty()).map_or_else(|| unknown.clone(), |rate| {
            interpolate(
                &t(locale, "toolbar.valBitRate", "{{val}} Mbps"),
                &[(
                    "val",
                    &format!(
                        "{:.0}",
                        rate.parse::<f64>().unwrap_or(f64::NAN) / 1_000_000.0
                    ),
                )],
            )
        });
    let telemetry = match video.telemetry.status() {
        TelemetryStatus::Pending => t(locale, "toolbar.telemetryPending", "Reading…"),
        TelemetryStatus::Ready => t(locale, "toolbar.telemetryReady", "Available"),
        TelemetryStatus::Absent => t(locale, "toolbar.telemetryAbsent", "None"),
        TelemetryStatus::Failed => t(locale, "toolbar.telemetryFailed", "Could not be read"),
    };
    let first = |values: [&Option<String>; 2]| {
        values
            .into_iter()
            .flatten()
            .find(|value| !value.is_empty())
            .cloned()
            .unwrap_or_else(|| unknown.clone())
    };
    let row = |key: &str, default: &'static str, value: String| MetadataRow {
        label: t(locale, key, default),
        value,
    };
    vec![
        row(
            "toolbar.duration",
            "Duration",
            format_clock_duration(clip.duration_seconds()),
        ),
        row(
            "toolbar.frameRate",
            "Frame rate",
            interpolate(
                &t(locale, "toolbar.valFps", "{{val}} fps"),
                &[("val", &rate)],
            ),
        ),
        row(
            "toolbar.resolution",
            "Resolution",
            interpolate(
                &t(locale, "toolbar.valResolution", "{{width}}×{{height}}"),
                &[
                    ("width", &video.resolution.width.to_string()),
                    ("height", &video.resolution.height.to_string()),
                ],
            ),
        ),
        row(
            "toolbar.createdAt",
            "Created at",
            format_video_creation_time(
                video.creation_time.as_deref(),
                video.time_source.as_deref(),
                timezone,
                clip.edits.timezone_mode,
                locale,
            ),
        ),
        row("toolbar.timeSource", "Time source", time_source),
        row(
            "toolbar.codec",
            "Codec",
            first([&video.codec_name, &video.codec_long_name]),
        ),
        row("toolbar.bitRate", "Bit rate", bit_rate),
        row(
            "toolbar.camera",
            "Camera",
            first([&video.camera_model, &video.camera_type]),
        ),
        row("toolbar.telemetry", "Telemetry", telemetry),
    ]
}

/// Which side a removed part joins.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MergeSide {
    Previous,
    Next,
}

/// One part of the selected video.
#[derive(Clone, Debug, PartialEq)]
pub struct SegmentRow {
    pub key: String,
    pub start_label: String,
    pub end_label: String,
    /// The timeline second pressing the range seeks to.
    pub seek_second: f64,
    pub included: bool,
    pub shows_overlay: bool,
    /// The join into this part, when it joins a part of the same video.
    pub join: Option<JoinModel>,
    /// The cut a removal drops, the part kept, and the side it joins.
    pub removal: Option<(String, String, MergeSide)>,
}

/// A join out of the selected video into the next one.
#[derive(Clone, Debug, PartialEq)]
pub struct ExitRow {
    pub label: String,
    pub next_filename: String,
    pub join: JoinModel,
}

/// The video drawer's row of the selected clip.
#[derive(Clone, Debug, PartialEq)]
pub struct SelectedVideoRow {
    pub id: String,
    pub filename: String,
    pub path: String,
    pub missing: bool,
    pub warnings: Vec<String>,
    pub metadata: Vec<MetadataRow>,
    pub creation_time_source: CreationTimeSource,
    pub filename_creation_time_available: bool,
    /// Apply Timezone shows only with the activity's timezone.
    pub timezone_shown: bool,
    pub applies_timezone: bool,
    pub timezone_disabled: bool,
    pub segments: Vec<SegmentRow>,
    pub exits: Vec<ExitRow>,
}

/// The joins between parts of one video, and the joins out of each video.
fn group_joins(program: &Program) -> (BTreeSet<String>, BTreeMap<String, Vec<String>>) {
    let mut parts = BTreeSet::new();
    let mut exits: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for join in &program.joins {
        let from_video = &program.piece(&join.from_piece_id).video_id;
        if *from_video == join.video_id {
            parts.insert(join.key.clone());
        } else {
            exits
                .entry(from_video.clone())
                .or_default()
                .push(join.key.clone());
        }
    }
    (parts, exits)
}

/// The id of the video the drawer shows: the selected clip's.
pub fn selected_video_id(store: &Store) -> Option<&str> {
    store
        .video_import
        .selected_clip
        .as_ref()
        .and_then(|selection| selection.lane.video_id())
}

/// The selected clip's row, or `None` when no video is selected.
pub fn selected_video_row(store: &Store) -> Option<SelectedVideoRow> {
    let locale = store.i18n.language;
    let timeline = store.player.timeline.timeline();
    let committed = &timeline.committed;
    let clip = committed
        .clips
        .iter()
        .find(|clip| Some(clip.id()) == selected_video_id(store))?;
    let timezone = store
        .media
        .parsed_activity
        .as_ref()
        .and(store.media.activity_summary.as_ref())
        .and_then(|summary| summary.timezone.as_deref());
    let models: BTreeMap<String, JoinModel> = join_models(committed, locale)
        .into_iter()
        .map(|model| (model.key.clone(), model))
        .collect();
    let (part_keys, exits) = group_joins(&committed.program);
    let names: BTreeMap<&str, String> = committed
        .clips
        .iter()
        .map(|clip| {
            (
                clip.id(),
                filename_from_selected_path(&clip.video.path)
                    .unwrap_or_default()
                    .to_string(),
            )
        })
        .collect();
    let segments = list_video_segments(clip);
    let rows = segments
        .iter()
        .enumerate()
        .map(|(index, segment)| {
            let start = clip.fps().frames_to_seconds(segment.start_frame);
            let end = clip.fps().frames_to_seconds(segment.end_frame);
            let join_key = segment_join_key(clip.id(), &segment.key);
            let removal = if index > 0 {
                Some((
                    segment.key.clone(),
                    segments[index - 1].key.clone(),
                    MergeSide::Previous,
                ))
            } else if segments.len() > 1 {
                Some((
                    segments[1].key.clone(),
                    segments[1].key.clone(),
                    MergeSide::Next,
                ))
            } else {
                None
            };
            SegmentRow {
                key: segment.key.clone(),
                start_label: format_clock_duration(start),
                end_label: format_clock_duration(end),
                seek_second: clip.offset_seconds() + start,
                included: !segment.edits.excluded,
                shows_overlay: !segment.edits.skip_overlay,
                join: part_keys
                    .contains(&join_key)
                    .then(|| models.get(&join_key).cloned())
                    .flatten(),
                removal,
            }
        })
        .collect();
    let warnings = timeline.warnings.get(clip.id())
        .map(|warnings| {
            warnings
                .iter()
                .map(|warning| format_video_warning(warning, locale))
                .collect()
        })
        .unwrap_or_default();
    Some(SelectedVideoRow {
        id: clip.id().to_string(),
        filename: names[clip.id()].clone(),
        path: clip.video.path.clone(),
        missing: clip.video.missing,
        warnings,
        metadata: metadata_rows(clip, timezone, locale),
        creation_time_source: if clip.video.time_source.as_deref() == Some("filename") {
            CreationTimeSource::Filename
        } else {
            CreationTimeSource::Detected
        },
        filename_creation_time_available: parse_video_filename_creation_time(&clip.video.path)
            .is_some(),
        timezone_shown: timezone.is_some(),
        applies_timezone: clip.edits.timezone_mode == Some(TimezoneMode::Utc),
        timezone_disabled: clip.video.time_source.as_deref() == Some("filename"),
        segments: rows,
        exits: exits
            .get(clip.id())
            .into_iter()
            .flatten()
            .filter_map(|key| models.get(key))
            .map(|model| ExitRow {
                label: interpolate(
                    &t(locale, "toolbar.transitionTo", "Transition to {{name}}"),
                    &[("name", &names[model.video_id.as_str()])],
                ),
                next_filename: names[model.video_id.as_str()].clone(),
                join: model.clone(),
            })
            .collect(),
    })
}

/// A card of the video panel.
#[derive(Clone, Debug, PartialEq)]
pub struct VideoCard {
    pub id: String,
    pub filename: String,
    pub duration_label: String,
    pub thumbnail: Option<Arc<[u8]>>,
    pub missing: bool,
    pub selected: bool,
    /// In Sync Doctor, the video whose landmarks hold the sync.
    pub holds_sync: bool,
}

/// The clips in timeline order.
pub fn video_cards(store: &Store) -> Vec<VideoCard> {
    let clips = &store.player.timeline.timeline().committed.clips;
    let selected = selected_video_id(store);
    let sync_target = store.manual_video_sync.video_id.as_deref().filter(|_| video_sync_mode(store));
    sort_clips_by_timeline(clips)
        .into_iter()
        .map(|clip| VideoCard {
            id: clip.id().to_string(),
            filename: filename_from_selected_path(&clip.video.path)
                .unwrap_or_default()
                .to_string(),
            duration_label: format_clock_duration(clip.duration_seconds()),
            thumbnail: clip.video.thumbnail.clone(),
            missing: clip.video.missing,
            selected: selected == Some(clip.id()),
            holds_sync: sync_target == Some(clip.id()),
        })
        .collect()
}

/// A card selects its whole clip. In Sync Doctor, which acts on the selected video, it also brings
/// the playhead into the clip.
pub fn select_card(store: &mut Store, video_id: &str) -> Result<(), String> {
    if video_sync_mode(store) {
        return select_clip_revealing_playhead(store, video_id);
    }
    select_clip(store, Some(SelectedClip { lane: ClipLane::Video(video_id.to_string()), segment_key: None }))
}

/// The file dialog, then the relink of the chosen file.
pub fn open_locate_dialog(store: &mut Store, services: &mut Services, video_id: String) {
    let opened = open_single_path(
        services,
        &[DialogFilter::new("Video", &VIDEO_EXTENSIONS)],
        DialogOptions {
            default_path: None,
            last_directory_key: Some(LAST_VIDEO_IMPORT_DIR_KEY),
        },
        Box::new(move |store, services, selected| {
            if let Some(path) = selected {
                locate_missing_video(store, services, video_id, path, None);
            }
        }),
    );
    if let Err(error) = opened {
        store
            .media
            .set_error_message(format!("Locating the video failed: {error}"));
    }
}

/// Probes the file under the missing video's identity and replaces it,
/// keeping its edits (cuts past a shorter file's end go). Its earlier edits leave the history, and
/// its telemetry is read in the background. An agent's call answers with the video, or the error.
pub fn locate_missing_video(
    store: &mut Store,
    services: &mut Services,
    video_id: String,
    path: String,
    reply: Option<crate::store::agent::AgentReply>,
) {
    let Some(missing) = store
        .video_import
        .imported_videos
        .iter()
        .find(|video| video.id == video_id && video.missing)
    else {
        let error = format!("Video is not missing: {video_id}");
        match reply {
            Some(reply) => reply.send(Err(error)),
            None => store
                .media
                .set_error_message(format!("Locating the video failed: {error}")),
        }
        return;
    };
    let import_order = missing.import_order;
    store.media.importing_video = true;
    let tools = Arc::clone(&services.video_tools);
    let unsupported = t(
        store.i18n.language,
        "video-preview.unsupportedVideoFile",
        "Not a supported video file (MP4, MOV, MKV).",
    );
    services.workers.spawn("video-locate", move || {
        let probed = probe_video(&*tools, &path, video_id.clone(), import_order, &unsupported);
        Box::new(move |store: &mut Store, services: &mut Services| {
            store.media.importing_video = false;
            let result = probed.and_then(|video| {
                crate::features::history::change_video_collection(
                    store,
                    &[video_id.as_str()],
                    |store| crate::store::videos::relink_video(store, video),
                )
            });
            match result {
                Ok(()) => {
                    queue_telemetry_extraction(store, services, std::slice::from_ref(&video_id));
                    if let Some(reply) = reply {
                        reply.send(Ok(serde_json::json!({ "id": video_id, "path": path })));
                    }
                }
                Err(error) => match reply {
                    Some(reply) => reply.send(Err(error)),
                    None => store
                        .media
                        .set_error_message(format!("Locating the video failed: {error}")),
                },
            }
        })
    });
}

/// `VideoPanel` takes the control panel's place while the video drawer is open, and in Sync Doctor.
pub fn video_panel_shown(store: &Store) -> bool {
    video_sync_mode(store)
        || (store.layout.left_drawer_visible && store.layout.active_left_drawer_tool == crate::features::toolbar::LeftDrawerTool::Video)
}

/// A part's range pressed: the playhead pauses at its start.
pub fn seek_segment(store: &mut Store, video_id: &str, segment_key: &str) -> Result<(), String> {
    let row = selected_video_row(store).filter(|row| row.id == video_id).ok_or_else(|| format!("Video {video_id} is not selected"))?;
    let segment = row.segments.iter().find(|segment| segment.key == segment_key).ok_or_else(|| format!("Unknown video segment: {segment_key}"))?;
    store.editor.commit_preview_scrub(segment.seek_second);
    Ok(())
}

/// A part's remove button: its cut goes, the merged part keeping the neighbour's state.
pub fn remove_segment(store: &mut Store, video_id: &str, segment_key: &str) -> Result<(), String> {
    let row = selected_video_row(store).filter(|row| row.id == video_id).ok_or_else(|| format!("Video {video_id} is not selected"))?;
    let segment = row.segments.iter().find(|segment| segment.key == segment_key).ok_or_else(|| format!("Unknown video segment: {segment_key}"))?;
    let (cut_id, keep, _) = segment.removal.clone().ok_or("A video with one part has no cut to remove")?;
    crate::store::videos::request_cut_removal(store, video_id, &cut_id, Some(&keep)).map(|_| ())
}

/// The drawer's tools.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum LeftDrawerTool {
    Projects,
    Activity,
    Video,
    VideoSync,
    #[default]
    Widgets,
}

impl LeftDrawerTool {
    /// The rail's order.
    pub const ALL: [LeftDrawerTool; 5] = [
        Self::Projects,
        Self::Activity,
        Self::Video,
        Self::VideoSync,
        Self::Widgets,
    ];
}

/// Sync Doctor's tabs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VideoSyncDrawerTab {
    #[default]
    Autodetect,
    Manual,
}

/// The durable drawer preference (`leftDrawer`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DrawerPreference {
    pub pinned: bool,
    pub active_tool: LeftDrawerTool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LayoutState {
    pub left_drawer_initialized: bool,
    pub left_drawer_visible: bool,
    pub left_drawer_pinned: bool,
    pub active_left_drawer_tool: LeftDrawerTool,
    pub video_sync_drawer_tab: VideoSyncDrawerTab,
}

impl LayoutState {
    /// Establishes canonical drawer state from a normalized preference.
    pub fn initialize_left_drawer(&mut self, preference: DrawerPreference) {
        self.left_drawer_initialized = true;
        self.left_drawer_pinned = preference.pinned;
        self.left_drawer_visible = preference.pinned;
        self.active_left_drawer_tool = preference.active_tool;
    }

    /// Selects a tool and applies the shared drawer visibility rules: selecting the tool already
    /// shown closes an unpinned drawer.
    pub fn select_left_drawer_tool(&mut self, tool: LeftDrawerTool) {
        if self.left_drawer_pinned {
            self.active_left_drawer_tool = tool;
            self.left_drawer_visible = true;
            return;
        }
        if self.left_drawer_visible && self.active_left_drawer_tool == tool {
            self.left_drawer_visible = false;
            return;
        }
        self.active_left_drawer_tool = tool;
        self.left_drawer_visible = true;
    }

    /// Opens Sync Doctor directly on its manual-sync tab.
    pub fn open_manual_video_sync(&mut self) {
        self.active_left_drawer_tool = LeftDrawerTool::VideoSync;
        self.left_drawer_visible = true;
        self.video_sync_drawer_tab = VideoSyncDrawerTab::Manual;
    }

    pub fn set_video_sync_drawer_tab(&mut self, tab: VideoSyncDrawerTab) {
        self.video_sync_drawer_tab = tab;
    }

    /// Dismisses the drawer only while it is a temporary workspace overlay.
    pub fn dismiss_left_drawer_overlay(&mut self) {
        if !self.left_drawer_pinned {
            self.left_drawer_visible = false;
        }
    }

    /// Changes drawer mode while preserving a visible active drawer.
    pub fn set_left_drawer_pinned(&mut self, pinned: bool) {
        self.left_drawer_pinned = pinned;
        self.left_drawer_visible = true;
    }

    /// The drawer is shown as a temporary overlay over the workspace.
    pub fn is_unpinned_overlay(&self) -> bool {
        self.left_drawer_visible && !self.left_drawer_pinned
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::services::fakes::fake_services;
    use crate::store::Dispatcher;
    use serde_json::json;
    use crate::features::player::reconcile_player;
    use crate::video::fixtures::*;
    use crate::store::videos::tests::store_with;
    use crate::store::videos::{cut_video, select_clip, ClipLane, SelectedClip};

    #[test]
    fn normalizes_malformed_preferences_to_the_default() {
        for value in [
            json!(null),
            json!([]),
            json!("widgets"),
            json!({"pinned": "yes", "activeTool": "video"}),
            json!({"pinned": true, "activeTool": "timeline"}),
            json!({"pinned": true}),
        ] {
            assert_eq!(
                normalize_drawer_preference(Some(&value)),
                DrawerPreference::default(),
                "{value}"
            );
        }
        assert_eq!(
            normalize_drawer_preference(None),
            DrawerPreference::default()
        );
        assert_eq!(
            normalize_drawer_preference(Some(&json!({"pinned": true, "activeTool": "videoSync"}))),
            DrawerPreference {
                pinned: true,
                active_tool: LeftDrawerTool::VideoSync
            }
        );
    }

    #[test]
    fn the_drawer_preference_is_written_once_per_change_and_not_at_hydration() {
        let (parts, fakes) =
            fake_services(json!({"leftDrawer": {"pinned": true, "activeTool": "video"}}));
        let store = Store::hydrate(&parts.preferences).unwrap();
        assert!(store.layout.left_drawer_visible);
        assert_eq!(store.layout.active_left_drawer_tool, LeftDrawerTool::Video);
        let dispatcher = Dispatcher::new(store, parts);
        let saved_drawer = || {
            fakes
                .preferences
                .saves
                .borrow()
                .iter()
                .map(|saved| saved[DRAWER_PREFERENCE_KEY].clone())
                .collect::<Vec<_>>()
        };

        dispatcher.dispatch(|_, _| {});
        assert!(saved_drawer().is_empty(), "nothing changed since hydration");

        dispatcher.dispatch(|store, _| {
            store
                .layout
                .select_left_drawer_tool(LeftDrawerTool::Widgets);
        });
        dispatcher.dispatch(|store, _| store.layout.dismiss_left_drawer_overlay());
        dispatcher.dispatch(|store, _| store.layout.set_left_drawer_pinned(false));
        dispatcher.dispatch(|store, _| store.layout.dismiss_left_drawer_overlay());

        assert_eq!(
            saved_drawer(),
            [
                json!({"pinned": true, "activeTool": "widgets"}),
                json!({"pinned": false, "activeTool": "widgets"})
            ]
        );
    }

    #[test]
    fn escape_dismisses_only_an_unpinned_drawer() {
        let mut store = Store::default();
        let escape = KeyInput::new("escape");
        assert!(!handle_drawer_capture_key(&mut store, &escape), "a hidden drawer leaves Escape alone");

        store.layout.select_left_drawer_tool(LeftDrawerTool::Activity);
        assert!(handle_drawer_capture_key(&mut store, &escape));
        assert!(!store.layout.left_drawer_visible);

        store.layout.set_left_drawer_pinned(true);
        assert!(!handle_drawer_capture_key(&mut store, &escape), "a pinned drawer leaves Escape to the focused popup");
        assert!(store.layout.left_drawer_visible);
    }

    #[test]
    fn other_keys_do_not_close_the_drawer() {
        let mut store = Store::default();
        store.layout.select_left_drawer_tool(LeftDrawerTool::Activity);
        let mut shifted = KeyInput::new("escape");
        shifted.shift = true;
        assert!(!handle_drawer_capture_key(&mut store, &shifted));
        assert!(!handle_drawer_capture_key(&mut store, &KeyInput::new("x")));
        assert!(store.layout.left_drawer_visible);
    }

    #[test]
    fn the_activity_drawer_formats_details_metrics_and_laps() {
        use crate::store::media::{ActivityLap, AvailableMetric};
        let mut store = Store::default();
        assert!(activity_drawer_view(&store).is_none());
        store.media.activity_source = Some(ActivitySource::File { path: "/rides/day 2.fit".into() });
        store.media.parsed_activity = Some(crate::store::media::ImportedActivity {
            activity: std::sync::Arc::new(Default::default()),
            preview_bytes: std::sync::Arc::new(Vec::new()),
        });
        store.media.activity_summary = Some(ActivitySummary {
            available_metrics: vec![
                AvailableMetric { attribute: "speed".into(), source: MetricSource::Direct },
                AvailableMetric { attribute: "gradient".into(), source: MetricSource::Mixed },
            ],
            duration_seconds: 3725.0,
            end_time: None,
            file_format: Some("fit".into()),
            file_name: Some("x.fit".into()),
            laps: vec![ActivityLap { id: 0, start_seconds: 0.0, duration_seconds: 61.234 }, ActivityLap { id: 1, start_seconds: 61.2, duration_seconds: 3700.0 }],
            original_sample_count: None,
            sample_count: 10.0,
            sport: None,
            sync_time: None,
            timezone: None,
            total_distance_meters: 999.6,
        });
        store.media.disabled_laps = vec![1];
        let view = activity_drawer_view(&store).unwrap();
        assert_eq!(view.file_name, "day 2.fit");
        assert!(view.can_delete);
        assert_eq!(view.format_label, "FIT");
        assert_eq!(view.metadata[0].value, "Unknown");
        assert_eq!(view.metadata[3].value, "1:02:05");
        assert!(view.metadata[2].value.ends_with(" m"));
        assert_eq!(view.metadata.len(), 5, "no extracted-points row without a count");
        assert_eq!(view.extracted.len(), 1);
        assert_eq!(view.derived.len(), 1, "mixed metrics count as calculated");
        assert_eq!((view.laps[0].duration.as_str(), view.laps[1].duration.as_str()), ("01:01.23", "01:01:40.00"));
        assert_eq!(view.laps[1].start, "01:01");
        assert!(!view.laps[1].enabled);
    }

    fn selected(store: &mut Store, id: &str) {
        select_clip(
            store,
            Some(SelectedClip {
                lane: ClipLane::Video(id.into()),
                segment_key: None,
            }),
        )
        .unwrap();
        reconcile_player(store);
    }

    #[test]
    fn the_drawer_shows_the_selected_clip_with_its_parts_and_exits() {
        let mut store = store_with(
            vec![
                simple("a", 10.0, 0),
                video(TestVideo {
                    id: "b",
                    path: "/videos/b.mp4",
                    duration_seconds: 10.0,
                    import_order: 1,
                    ..TestVideo::default()
                }),
            ],
            &[("b", 10.0)],
        );
        cut_video(&mut store, "a", 150).unwrap();
        reconcile_player(&mut store);
        assert_eq!(selected_video_row(&store), None, "nothing is selected");
        selected(&mut store, "a");
        let row = selected_video_row(&store).unwrap();
        assert_eq!(row.segments.len(), 2);
        assert_eq!(
            (
                row.segments[0].start_label.as_str(),
                row.segments[0].end_label.as_str()
            ),
            ("00:00", "00:05")
        );
        assert_eq!(
            row.segments[0].removal.as_ref().map(|removal| removal.2),
            Some(MergeSide::Next)
        );
        assert_eq!(
            row.segments[1].removal.as_ref().map(|removal| removal.2),
            Some(MergeSide::Previous)
        );
        assert!(
            row.segments[1].join.is_some(),
            "the join into the second part"
        );
        assert_eq!(row.exits.len(), 1);
        assert_eq!(row.exits[0].next_filename, "b.mp4");
        assert_eq!(row.metadata[0].value, "00:10");
        assert_eq!(row.metadata[8].value, "Reading…");
    }

    #[test]
    fn cards_follow_the_timeline_and_mark_the_selection() {
        let mut store = store_with(
            vec![simple("late", 10.0, 0), simple("early", 10.0, 1)],
            &[("late", 30.0)],
        );
        selected(&mut store, "late");
        let cards = video_cards(&store);
        assert_eq!(
            cards
                .iter()
                .map(|card| (card.id.as_str(), card.selected))
                .collect::<Vec<_>>(),
            [("early", false), ("late", true)]
        );
    }

    #[test]
    fn in_sync_doctor_the_panel_marks_the_card_that_holds_the_sync_and_a_card_brings_the_playhead_in() {
        let mut store = store_with(
            vec![simple("late", 10.0, 0), simple("early", 10.0, 1)],
            &[("late", 30.0)],
        );
        crate::features::sync::manual::add_landmark(&mut store, "late", ovrley_core::media::sync::LandmarkKind::Stop, 1.0).unwrap();
        reconcile_player(&mut store);
        let holds = |store: &Store| video_cards(store).into_iter().map(|card| (card.id, card.holds_sync)).collect::<Vec<_>>();
        assert_eq!(holds(&store), [("early".to_string(), false), ("late".to_string(), false)], "only in Sync Doctor");
        assert!(!video_panel_shown(&store));
        select_card(&mut store, "late").unwrap();
        assert_eq!(store.editor.selected_second, 0.0, "outside Sync Doctor the playhead stays");
        store.layout.select_left_drawer_tool(LeftDrawerTool::VideoSync);
        assert!(video_panel_shown(&store));
        assert_eq!(holds(&store), [("early".to_string(), false), ("late".to_string(), true)]);
        select_card(&mut store, "late").unwrap();
        assert_eq!(store.editor.selected_second, 30.0);
    }

    fn initialized(pinned: bool) -> LayoutState {
        let mut state = LayoutState::default();
        state.initialize_left_drawer(DrawerPreference {
            pinned,
            active_tool: LeftDrawerTool::Widgets,
        });
        state
    }

    #[test]
    fn initialization_restores_a_pinned_drawer_visibly_with_its_active_tool() {
        let state = initialized(true);
        assert_eq!(state.active_left_drawer_tool, LeftDrawerTool::Widgets);
        assert!(state.left_drawer_initialized);
        assert!(state.left_drawer_pinned);
        assert!(state.left_drawer_visible);
    }

    #[test]
    fn selecting_the_active_tool_toggles_only_an_unpinned_drawer() {
        let mut state = initialized(false);

        state.select_left_drawer_tool(LeftDrawerTool::Widgets);
        assert!(state.left_drawer_visible);

        state.select_left_drawer_tool(LeftDrawerTool::Widgets);
        assert!(!state.left_drawer_visible);

        state.set_left_drawer_pinned(true);
        state.select_left_drawer_tool(LeftDrawerTool::Widgets);
        assert!(state.left_drawer_visible);
    }

    #[test]
    fn dismissal_and_unpinning_preserve_the_canonical_visibility_rules() {
        let mut state = initialized(true);

        state.dismiss_left_drawer_overlay();
        assert!(state.left_drawer_visible);

        state.set_left_drawer_pinned(false);
        assert!(!state.left_drawer_pinned);
        assert!(state.left_drawer_visible);

        state.dismiss_left_drawer_overlay();
        assert!(!state.left_drawer_visible);
    }

    #[test]
    fn opens_sync_doctor_directly_on_manual_sync() {
        let mut state = LayoutState::default();

        state.open_manual_video_sync();

        assert_eq!(state.active_left_drawer_tool, LeftDrawerTool::VideoSync);
        assert!(state.left_drawer_visible);
        assert_eq!(state.video_sync_drawer_tab, VideoSyncDrawerTab::Manual);
    }

    #[test]
    fn accepts_only_canonical_sync_doctor_tabs() {
        let mut state = LayoutState::default();
        state.set_video_sync_drawer_tab(serde_json::from_str("\"manual\"").unwrap());
        assert_eq!(state.video_sync_drawer_tab, VideoSyncDrawerTab::Manual);
        assert!(serde_json::from_str::<VideoSyncDrawerTab>("\"settings\"").is_err());
    }

    #[test]
    fn tools_serialize_in_camel_case() {
        let names: Vec<String> = LeftDrawerTool::ALL
            .iter()
            .map(|tool| serde_json::to_string(tool).unwrap())
            .collect();
        assert_eq!(
            names,
            [
                "\"projects\"",
                "\"activity\"",
                "\"video\"",
                "\"videoSync\"",
                "\"widgets\""
            ]
        );
    }
}
