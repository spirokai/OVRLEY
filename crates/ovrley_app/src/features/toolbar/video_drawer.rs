//! Port of the video drawer's logic (plan D10): `useSelectedVideoRow`, `videoDrawerUtils.js`
//! (`buildVideoDrawerViewModel`) and `videoSegmentRows.js`. The drawer shows the clip selected on
//! the timeline or in the video panel; every command acts on that clip. Missing videos are located
//! here (`locateMissingVideo`).

use crate::features::transitions::{join_models, JoinModel};
use crate::features::video_preview::import::{
    probe_video, queue_telemetry_extraction, LAST_VIDEO_IMPORT_DIR_KEY, VIDEO_EXTENSIONS,
};
use crate::i18n::{catalog, interpolate, Locale};
use crate::lib::file_dialog::{filename_from_selected_path, open_single_path, DialogOptions};
use crate::lib::format::{format_clock_duration, format_video_creation_time};
use crate::lib::video_program::collection::sort_clips_by_timeline;
use crate::lib::video_program::edits::CreationTimeSource;
use crate::lib::video_program::program::Program;
use crate::lib::video_program::segments::{list_video_segments, segment_join_key};
use crate::lib::video_program::warnings::format_video_warning;
use crate::lib::video_program::{Clip, TelemetryStatus};
use crate::lib::video_sync::parse_video_filename_creation_time;
use crate::store::services::{DialogFilter, Services};
use crate::store::Store;
use ovrley_core::media::video_edits::TimezoneMode;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

fn t(locale: Locale, key: &str, default: &'static str) -> String {
    catalog().t_or(locale, key, default).to_string()
}

/// One row of the details list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetadataRow {
    pub label: String,
    pub value: String,
}

/// `buildVideoDrawerViewModel`'s rows: duration, frame rate, resolution, created at, time source,
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
    let rate = crate::js::number_to_string(crate::js::round(video.fps.as_f64() * 100.0) / 100.0);
    let bit_rate = video
        .bit_rate
        .as_deref()
        .filter(|rate| !rate.is_empty())
        .map(|rate| {
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
        })
        .unwrap_or_else(|| unknown.clone());
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

/// One part of the selected video (`buildVideoSegmentRows`).
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

/// `useSelectedVideoRow`'s row.
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

/// `groupVideoJoins`: the joins between parts of one video, and the joins out of each video.
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

/// `useSelectedVideoRow`: the selected clip's row, or `None` when no video is selected.
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

/// A card of the video panel (`useVideoCards`).
#[derive(Clone, Debug, PartialEq)]
pub struct VideoCard {
    pub id: String,
    pub filename: String,
    pub duration_label: String,
    pub thumbnail: Option<Arc<[u8]>>,
    pub missing: bool,
    pub selected: bool,
}

/// `useVideoCards`: the clips in timeline order.
pub fn video_cards(store: &Store) -> Vec<VideoCard> {
    let clips = &store.player.timeline.timeline().committed.clips;
    let selected = selected_video_id(store);
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
        })
        .collect()
}

/// `locateVideo`: the file dialog, then the relink of the chosen file.
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

/// `locateMissingVideo`: probes the file under the missing video's identity and replaces it,
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
                crate::features::undo_redo::change_video_collection(
                    store,
                    &[video_id.as_str()],
                    |store| crate::store::slices::video_import::relink_video(store, video),
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

/// `VideoPanel` takes the control panel's place while the video drawer is open.
pub fn video_panel_shown(store: &Store) -> bool {
    store.layout.left_drawer_visible && store.layout.active_left_drawer_tool == crate::store::slices::layout::LeftDrawerTool::Video
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
    crate::store::slices::video_import::request_cut_removal(store, video_id, &cut_id, Some(&keep)).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::player::reconcile_player;
    use crate::lib::video_program::fixtures::*;
    use crate::store::slices::video_import::tests::store_with;
    use crate::store::slices::video_import::{cut_video, select_clip, ClipLane, SelectedClip};

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
}
