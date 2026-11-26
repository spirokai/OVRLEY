//! The project document the session saves, and the content the project status compares with the
//! saved baseline.
//!
//! The document is `ovrley_services::project`'s own type, so what is saved is what the store
//! holds. Sources are located relative to the project when they sit in its folder.

use std::collections::BTreeMap;
use ovrley_core::activity::trim::ActivityTrim;
use ovrley_core::encode::ffmpeg::transitions::TransitionDefaults;
use ovrley_core::encode::settings::{ExportMode, OutputMode};
use ovrley_services::project::{create_path_locator, PROJECT_FORMAT, PROJECT_VERSION, PathLocator, ProjectDocument, ProjectEditor, ProjectLaps, ProjectRender, ProjectSource, ProjectSources, ProjectSync, ProjectTimeline, ProjectVideo};
use ovrley_widgets::model::document::{canonical_numbers, number};
use serde_json::Value;
use crate::features::templates::durable_template_state;
use crate::store::Store;
use crate::store::media::ActivitySource;
use crate::video::video_metadata_snapshot;

/// The preference that remembers the project folder.
pub const LAST_PROJECT_DIRECTORY_KEY: &str = "last-project-dir";

/// The project content that takes part in the status: everything a project saves except the
/// envelope, the raster assets and the timeline.
#[derive(Clone, Debug, PartialEq)]
pub struct ProjectContent {
    pub editor: ProjectEditor,
    pub sources: ProjectSources,
    pub sync: ProjectSync,
    pub laps: ProjectLaps,
    pub activity_trim: Option<ActivityTrim>,
    pub transition_defaults: TransitionDefaults,
    pub render: ProjectRender,
}

/// The saved baseline: the content and the timeline as last saved.
#[derive(Clone, Debug, PartialEq)]
pub struct ProjectBaseline {
    pub content: ProjectContent,
    pub timeline: ProjectTimeline,
}

impl ProjectBaseline {
    pub fn of(document: ProjectDocument) -> Self {
        Self {
            content: ProjectContent {
                editor: document.editor,
                sources: document.sources,
                sync: document.sync,
                laps: document.laps,
                activity_trim: document.activity_trim,
                transition_defaults: document.transition_defaults,
                render: document.render,
            },
            timeline: document.timeline,
        }
    }
}

/// A source locator relative to the project, or absolute for a project never saved.
fn locate_path(path: &str, project_path: Option<&str>) -> Result<PathLocator, String> {
    match project_path {
        Some(project_path) => create_path_locator(path, project_path),
        None => Ok(PathLocator::Absolute(path.to_string())),
    }
}

/// The videos in import order: identity, locator, edits, and the metadata that keeps a video on
/// the timeline when its file cannot be found.
fn project_videos(store: &Store, project_path: Option<&str>) -> Result<Vec<ProjectVideo>, String> {
    let mut videos: Vec<_> = store.video_import.imported_videos.iter().collect();
    videos.sort_by_key(|video| video.import_order);
    videos
        .into_iter()
        .map(|video| {
            Ok(ProjectVideo {
                id: video.id.clone(),
                path: locate_path(&video.path, project_path)?,
                edits: store
                    .video_import
                    .video_edits
                    .get(&video.id)
                    .ok_or_else(|| format!("Video {} has no edits", video.id))?
                    .clone(),
                metadata: Some(video_metadata_snapshot(video)),
            })
        })
        .collect()
}

/// The editor's durable state, with the scene's fps and update rate from the render settings.
fn project_editor(store: &Store) -> Result<ProjectEditor, String> {
    let durable = durable_template_state(store)?;
    let mut config = durable.config.to_value();
    let scene = config
        .get_mut("scene")
        .and_then(Value::as_object_mut)
        .ok_or("The durable config has no scene")?;
    scene.insert("fps".into(), number(store.render_settings.fps));
    scene.insert(
        "updateRate".into(),
        Value::from(store.render_settings.widget_update_rate),
    );
    canonical_numbers(config.as_object_mut().expect("a config is an object"));
    Ok(ProjectEditor {
        config,
        global_defaults: durable.global_defaults.clone(),
    })
}

/// The project content the status compares. The export mode is `transparent` unless the output is separate
/// or there are videos.
pub fn project_content(
    store: &Store,
    project_path: Option<&str>,
) -> Result<ProjectContent, String> {
    let activity = match &store.media.activity_source {
        Some(ActivitySource::File { path }) => Some(ProjectSource {
            path: locate_path(path, project_path)?,
        }),
        None => None,
    };
    let mut settings = store.render_settings.clone();
    if settings.output_mode != OutputMode::Separate && !store.video_import.has_videos() {
        settings.export_mode = ExportMode::Transparent;
    }
    Ok(ProjectContent {
        editor: project_editor(store)?,
        sources: ProjectSources {
            activity,
            videos: project_videos(store, project_path)?,
        },
        sync: ProjectSync {
            manual: store.manual_video_sync.clone(),
        },
        laps: ProjectLaps {
            disabled: store.media.disabled_laps.clone(),
        },
        activity_trim: store.media.activity_trim,
        transition_defaults: store.video_import.transition_defaults.clone(),
        render: ProjectRender {
            settings,
            batch_output_folder: store.batch_render.batch_output_folder.clone(),
        },
    })
}

/// The playhead and the visible window.
pub fn project_timeline(store: &Store) -> ProjectTimeline {
    ProjectTimeline {
        playhead_second: store.editor.selected_second,
        view_start: store.editor.timeline_viewport.view_start,
        view_end: store.editor.timeline_viewport.view_end,
    }
}

/// The version 6 document of the session. The writer fills the raster
/// assets.
pub fn create_project_snapshot(
    store: &Store,
    project_path: Option<&str>,
) -> Result<ProjectDocument, String> {
    let content = project_content(store, project_path)?;
    Ok(ProjectDocument {
        format: PROJECT_FORMAT.into(),
        version: PROJECT_VERSION,
        saved_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        editor: content.editor,
        sources: content.sources,
        sync: content.sync,
        laps: content.laps,
        activity_trim: content.activity_trim,
        transition_defaults: content.transition_defaults,
        render: content.render,
        timeline: project_timeline(store),
        raster_assets: BTreeMap::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::video::fixtures::{video, TestVideo};
    use crate::video::VideoEditsMap;
    use ovrley_core::media::edits::VideoEdits;
    use std::rc::Rc;

    fn with_videos(paths: &[(&'static str, &'static str, u32)]) -> Store {
        let mut store = Store::default();
        let videos: Vec<_> = paths
            .iter()
            .map(|(id, path, import_order)| {
                video(TestVideo {
                    id,
                    path,
                    import_order: *import_order,
                    ..TestVideo::default()
                })
            })
            .collect();
        let edits: VideoEditsMap = videos
            .iter()
            .map(|video| (video.id.clone(), VideoEdits::default()))
            .collect();
        store.video_import.imported_videos = Rc::new(videos);
        store.video_import.video_edits = Rc::new(edits);
        store
    }

    #[test]
    fn saves_videos_in_import_order_relative_to_the_project_folder() {
        let store = with_videos(&[("b", "/work/race/b.mp4", 1), ("a", "/elsewhere/a.mp4", 0)]);
        let document = create_project_snapshot(&store, Some("/work/race/race.oly")).unwrap();
        let videos = &document.sources.videos;
        assert_eq!(
            videos
                .iter()
                .map(|video| video.id.as_str())
                .collect::<Vec<_>>(),
            ["a", "b"]
        );
        assert_eq!(
            videos[0].path,
            PathLocator::Absolute("/elsewhere/a.mp4".into())
        );
        assert_eq!(videos[1].path, PathLocator::ProjectRelative("b.mp4".into()));
        assert!(
            videos[0].metadata.is_some(),
            "the metadata keeps a missing video on the timeline"
        );
    }

    #[test]
    fn a_never_saved_project_keeps_absolute_sources() {
        let mut store = with_videos(&[("a", "/work/a.mp4", 0)]);
        store.media.activity_source = Some(ActivitySource::File {
            path: "/work/ride.fit".into(),
        });
        let content = project_content(&store, None).unwrap();
        assert_eq!(
            content.sources.activity.unwrap().path,
            PathLocator::Absolute("/work/ride.fit".into())
        );
        assert_eq!(
            content.sources.videos[0].path,
            PathLocator::Absolute("/work/a.mp4".into())
        );
    }

    #[test]
    fn the_scene_takes_the_render_fps_and_update_rate() {
        let mut store = Store::default();
        store
            .render_settings
            .set_fps_and_update_rate(25.0, 5)
            .unwrap();
        let content = project_content(&store, None).unwrap();
        assert_eq!(content.editor.config["scene"]["fps"], Value::from(25));
        assert_eq!(content.editor.config["scene"]["updateRate"], Value::from(5));
    }

    #[test]
    fn the_export_mode_is_transparent_without_videos_unless_separate() {
        let mut store = Store::default();
        assert_eq!(
            project_content(&store, None)
                .unwrap()
                .render
                .settings
                .export_mode,
            ExportMode::Transparent
        );
        store.render_settings.output_mode = OutputMode::Separate;
        assert_eq!(
            project_content(&store, None)
                .unwrap()
                .render
                .settings
                .export_mode,
            ExportMode::Composite
        );
        let store = with_videos(&[("a", "/a.mp4", 0)]);
        assert_eq!(
            project_content(&store, None)
                .unwrap()
                .render
                .settings
                .export_mode,
            ExportMode::Composite
        );
    }

    #[test]
    fn the_snapshot_round_trips_through_the_project_writer_shape() {
        let store = with_videos(&[("a", "/a.mp4", 0)]);
        let document = create_project_snapshot(&store, None).unwrap();
        let json = serde_json::to_value(&document).unwrap();
        assert_eq!(json["format"], "ovrley-project");
        assert_eq!(json["version"], 6);
        assert_eq!(json["rasterAssets"], serde_json::json!({}));
        assert_eq!(
            json["timeline"],
            serde_json::json!({"playheadSecond": 0.0, "viewStart": 0.0, "viewEnd": 73.0})
        );
        let back: ProjectDocument = serde_json::from_value(json).unwrap();
        assert_eq!(ProjectBaseline::of(back), ProjectBaseline::of(document));
    }
}
