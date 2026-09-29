//! Strict `.oly` project archive boundary.
//!
//! The project envelope is validated here before frontend orchestration sees it.
//! Widget config normalization belongs to the frontend project-load seam, while
//! archive and platform-path details do not leak into application state.

use crate::raster_resources::RasterResources;
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use chrono::DateTime;
use ovrley_core::normalize::{raw::RasterConfig, validate_template_rasters};
use ovrley_core::raster::{
    is_canonical_extension, load_embedded_raster, RasterError, SelectedRaster, MAX_ENCODED_BYTES,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use tauri::Manager;
use uuid::Uuid;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

const PROJECT_FORMAT: &str = "ovrley-project";
const PROJECT_VERSION: u32 = 3;
const PROJECT_VERSION_V1: u32 = 1;
const PROJECT_VERSION_V2: u32 = 2;
const MAX_MANUAL_LANDMARKS: usize = 5;
const MAX_MANUAL_LOCATION_LANDMARKS: usize = 1;
const DEFAULT_MANUAL_SPEED_THRESHOLD_KMH: f64 = 5.0;
const MIN_MANUAL_SPEED_THRESHOLD_KMH: f64 = 1.0;
const MAX_MANUAL_SPEED_THRESHOLD_KMH: f64 = 10.0;
const DEFAULT_MANUAL_TURN_THRESHOLD_DEGREES: f64 = 80.0;
const MIN_MANUAL_TURN_THRESHOLD_DEGREES: f64 = 70.0;
const MAX_MANUAL_TURN_THRESHOLD_DEGREES: f64 = 160.0;
const PROJECT_JSON_ENTRY: &str = "project.json";
const THUMBNAIL_ENTRY: &str = "thumbnail.png";
const RASTER_ASSET_NAMESPACE: &str = "rasters/";
const MAX_PROJECT_JSON_SIZE: u64 = 4 * 1024 * 1024;
const MAX_THUMBNAIL_SIZE: u64 = 2 * 1024 * 1024;
const MAX_ARCHIVE_SIZE: u64 = 40 * 1024 * 1024;
const THUMBNAIL_FILTER: &str =
    "scale=320:180:force_original_aspect_ratio=decrease,pad=320:180:(ow-iw)/2:(oh-ih)/2:color=black@0";

#[tauri::command]
pub(crate) fn default_project_directory(app: tauri::AppHandle) -> Result<String, String> {
    let directory = app
        .path()
        .document_dir()
        .map_err(|error| error.to_string())?
        .join("OVRLEY")
        .join("projects");
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    Ok(path_string(directory))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProjectFileSummary {
    name: String,
    path: String,
    thumbnail_data_url: Option<String>,
}

fn read_thumbnail_data_url(path: &Path) -> Option<String> {
    let file = File::open(path).ok()?;
    let mut archive = ZipArchive::new(file).ok()?;
    let mut entry = archive.by_name(THUMBNAIL_ENTRY).ok()?;
    if entry.size() > MAX_THUMBNAIL_SIZE {
        return None;
    }
    let mut bytes = Vec::with_capacity(entry.size() as usize);
    entry.read_to_end(&mut bytes).ok()?;
    Some(format!(
        "data:image/png;base64,{}",
        BASE64_STANDARD.encode(bytes)
    ))
}

#[tauri::command]
pub(crate) fn list_project_files(directory: String) -> Result<Vec<ProjectFileSummary>, String> {
    let directory = PathBuf::from(directory);
    if !directory.is_absolute() {
        return Err("Project directory must be an absolute path".into());
    }

    let mut projects = Vec::new();
    for entry in fs::read_dir(&directory).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        let is_project = entry
            .file_type()
            .map_err(|error| error.to_string())?
            .is_file()
            && path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("oly"));
        if !is_project {
            continue;
        }

        let name = path
            .file_stem()
            .and_then(|name| name.to_str())
            .ok_or_else(|| "Project filename must be valid UTF-8".to_string())?;
        projects.push(ProjectFileSummary {
            name: name.to_string(),
            thumbnail_data_url: read_thumbnail_data_url(&path),
            path: path_string(path),
        });
    }

    projects.sort_by(|left, right| {
        left.name
            .to_lowercase()
            .cmp(&right.name.to_lowercase())
            .then_with(|| left.path.cmp(&right.path))
    });
    Ok(projects)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ProjectDocument {
    format: String,
    version: u32,
    saved_at: String,
    editor: ProjectEditor,
    sources: ProjectSources,
    sync: ProjectSync,
    render: ProjectRender,
    timeline: ProjectTimeline,
    raster_assets: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProjectDocumentV2 {
    format: String,
    version: u32,
    saved_at: String,
    editor: ProjectEditor,
    sources: ProjectSources,
    sync: ProjectSync,
    render: ProjectRender,
    timeline: ProjectTimeline,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProjectDocumentV1 {
    format: String,
    version: u32,
    saved_at: String,
    editor: ProjectEditor,
    sources: ProjectSources,
    sync: ProjectSyncV1,
    render: ProjectRender,
    timeline: ProjectTimeline,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProjectEditor {
    config: Value,
    global_defaults: GlobalDefaults,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct GlobalDefaults {
    border_color: String,
    border_thickness: f64,
    shadow_color: String,
    shadow_strength: f64,
    shadow_distance: f64,
    font_values: String,
    font_text: String,
    color_values: String,
    color_text: String,
    color_icons: String,
    color_units: String,
    font_size: f64,
    opacity: f64,
    scale: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "kebab-case",
    deny_unknown_fields
)]
enum PathLocator {
    ProjectRelative(String),
    Absolute(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectSources {
    activity: Option<ProjectSource>,
    video: Option<ProjectSource>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectSource {
    path: PathLocator,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProjectSync {
    video_offset_seconds: f64,
    video_timezone_mode: Option<VideoTimezoneMode>,
    manual: ProjectManualVideoSync,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProjectSyncV1 {
    video_offset_seconds: f64,
    video_timezone_mode: Option<VideoTimezoneMode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProjectManualVideoSync {
    landmarks: Vec<ProjectLandmark>,
    #[serde(default)]
    detected_location_second: Option<f64>,
    speed_threshold_kmh: f64,
    turn_threshold_degrees: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", deny_unknown_fields)]
enum ProjectLandmark {
    #[serde(rename = "stop")]
    Stop {
        id: String,
        #[serde(rename = "videoSecond")]
        video_second: f64,
    },
    #[serde(rename = "leftTurn")]
    LeftTurn {
        id: String,
        #[serde(rename = "videoSecond")]
        video_second: f64,
    },
    #[serde(rename = "rightTurn")]
    RightTurn {
        id: String,
        #[serde(rename = "videoSecond")]
        video_second: f64,
    },
    #[serde(rename = "location")]
    Location {
        id: String,
        #[serde(rename = "videoSecond")]
        video_second: f64,
        #[serde(
            rename = "activitySecond",
            deserialize_with = "deserialize_required_activity_second"
        )]
        activity_second: Option<f64>,
    },
}

fn deserialize_required_activity_second<'de, D>(deserializer: D) -> Result<Option<f64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<f64>::deserialize(deserializer)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum VideoTimezoneMode {
    Local,
    Utc,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProjectRender {
    fps: f64,
    widget_update_rate: u32,
    export_mode: ExportMode,
    codec: String,
    bitrate_mbps: Option<f64>,
    range: ProjectRange,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum ExportMode {
    Transparent,
    Composite,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProjectRange {
    #[serde(rename = "type")]
    range_type: RangeType,
    from: f64,
    to: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum RangeType {
    All,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProjectTimeline {
    playhead_second: f64,
    view_start: f64,
    view_end: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ResolvedSources {
    activity_path: Option<String>,
    video_path: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReadProjectResult {
    project: ProjectDocument,
    resolved_sources: ResolvedSources,
    raster_load_results: HashMap<String, RasterLoadResult>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "lowercase")]
enum RasterLoadResult {
    Ready {
        #[serde(rename = "resourceId")]
        resource_id: String,
    },
    Error {
        #[serde(rename = "errorCode")]
        error_code: RasterError,
    },
}

struct ValidatedProject {
    document: ProjectDocument,
    populated_rasters: HashSet<String>,
}

struct LoadedArchive {
    project: ProjectDocument,
    rasters: HashMap<String, Result<SelectedRaster, RasterError>>,
}

struct RasterAsset {
    name: String,
    image: Arc<SelectedRaster>,
}

#[derive(Debug)]
enum ProjectFileError {
    Invalid(String),
    ArchiveSize,
    RasterUnavailable(String),
}

impl std::fmt::Display for ProjectFileError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(message) | Self::RasterUnavailable(message) => {
                formatter.write_str(message)
            }
            Self::ArchiveSize => {
                formatter.write_str("Project archive exceeds the 40 MiB size limit")
            }
        }
    }
}

impl From<String> for ProjectFileError {
    fn from(message: String) -> Self {
        Self::Invalid(message)
    }
}

impl ProjectFileError {
    fn command_error(self, default_code: &str) -> String {
        let code = match &self {
            Self::ArchiveSize => "archive_size",
            Self::RasterUnavailable(_) => "raster_unavailable",
            Self::Invalid(_) => default_code,
        };
        format!("[project_error:{code}] {self}")
    }
}

fn validate_locator(locator: &PathLocator) -> Result<(), String> {
    let (value, must_be_absolute) = match locator {
        PathLocator::ProjectRelative(value) => (value, false),
        PathLocator::Absolute(value) => (value, true),
    };
    if value.trim().is_empty() {
        return Err("Project path locator value must not be empty".into());
    }
    let path = Path::new(value);
    if must_be_absolute != path.is_absolute() {
        return Err(if must_be_absolute {
            "Absolute project locator must contain an absolute path".into()
        } else {
            "Project-relative locator must contain a relative path".into()
        });
    }
    if !must_be_absolute
        && path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err("Project-relative locator must remain inside the project directory".into());
    }
    Ok(())
}

fn validate_project(project: &ProjectDocument) -> Result<HashSet<String>, String> {
    if project.format != PROJECT_FORMAT {
        return Err(format!("Unsupported project format: {}", project.format));
    }
    if project.version != PROJECT_VERSION {
        return Err(format!("Unsupported project version: {}", project.version));
    }
    if DateTime::parse_from_rfc3339(&project.saved_at).is_err() {
        return Err("Project savedAt must be an ISO-8601 timestamp".into());
    }
    if let Some(source) = &project.sources.activity {
        validate_locator(&source.path)?;
    }
    if let Some(source) = &project.sources.video {
        validate_locator(&source.path)?;
    }
    for (label, value) in [
        ("sync.videoOffsetSeconds", project.sync.video_offset_seconds),
        ("render.fps", project.render.fps),
        ("render.range.from", project.render.range.from),
        ("render.range.to", project.render.range.to),
        ("timeline.playheadSecond", project.timeline.playhead_second),
        ("timeline.viewStart", project.timeline.view_start),
        ("timeline.viewEnd", project.timeline.view_end),
    ] {
        if !value.is_finite() {
            return Err(format!("{label} must be finite"));
        }
    }
    if project.render.fps <= 0.0 || project.render.widget_update_rate == 0 {
        return Err("Render fps and widgetUpdateRate must be positive".into());
    }
    if let Some(bitrate) = project.render.bitrate_mbps {
        if !bitrate.is_finite() || bitrate <= 0.0 {
            return Err("render.bitrateMbps must be a positive finite number or null".into());
        }
    }
    if project.render.codec.trim().is_empty() {
        return Err("render.codec must not be empty".into());
    }
    if matches!(project.render.export_mode, ExportMode::Composite)
        && project.sources.video.is_none()
    {
        return Err("Composite export mode requires a video source".into());
    }
    if matches!(project.render.range.range_type, RangeType::Custom)
        && project.render.range.from >= project.render.range.to
    {
        return Err("Custom render range requires from < to".into());
    }
    if project.timeline.view_start >= project.timeline.view_end {
        return Err("Timeline viewport requires viewStart < viewEnd".into());
    }
    if !project.sync.manual.landmarks.is_empty() && project.sources.video.is_none() {
        return Err("sync.manual.landmarks require a video source".into());
    }
    if project.sync.manual.detected_location_second.is_some() && project.sources.activity.is_none()
    {
        return Err("sync.manual.detectedLocationSecond requires an activity source".into());
    }
    validate_manual_video_sync(&project.sync.manual)?;
    validate_editor(&project.editor)?;
    validate_project_rasters(&project.editor.config, &project.raster_assets)
}

fn validate_project_rasters(
    config: &Value,
    mappings: &BTreeMap<String, String>,
) -> Result<HashSet<String>, String> {
    let values = config
        .get("rasters")
        .ok_or_else(|| "editor.config.rasters must be an array".to_string())?;
    let rasters: Vec<RasterConfig> = serde_json::from_value(values.clone())
        .map_err(|error| format!("Invalid project rasters: {error}"))?;
    let populated = validate_template_rasters(&rasters).map_err(|error| error.to_string())?;
    for (id, name) in mappings {
        if !populated.contains(id) {
            return Err(format!(
                "Raster asset mapping has no populated widget: {id}"
            ));
        }
        if !name
            .strip_prefix(RASTER_ASSET_NAMESPACE)
            .and_then(|name| name.strip_prefix(id))
            .and_then(|name| name.strip_prefix('.'))
            .is_some_and(is_canonical_extension)
        {
            return Err(format!("Invalid raster asset mapping for {id}"));
        }
    }
    Ok(populated)
}

fn validate_manual_video_sync(manual: &ProjectManualVideoSync) -> Result<(), String> {
    if let Some(activity_second) = manual.detected_location_second {
        if !activity_second.is_finite() || activity_second < 0.0 {
            return Err(
                "sync.manual.detectedLocationSecond must be a finite non-negative number or null"
                    .into(),
            );
        }
    }
    if manual.landmarks.len() > MAX_MANUAL_LANDMARKS {
        return Err(format!(
            "sync.manual.landmarks must contain at most {MAX_MANUAL_LANDMARKS} landmarks"
        ));
    }
    if !manual.speed_threshold_kmh.is_finite()
        || !(MIN_MANUAL_SPEED_THRESHOLD_KMH..=MAX_MANUAL_SPEED_THRESHOLD_KMH)
            .contains(&manual.speed_threshold_kmh)
    {
        return Err(format!(
            "sync.manual.speedThresholdKmh must be finite and between {MIN_MANUAL_SPEED_THRESHOLD_KMH} and {MAX_MANUAL_SPEED_THRESHOLD_KMH}"
        ));
    }
    if !manual.turn_threshold_degrees.is_finite()
        || !(MIN_MANUAL_TURN_THRESHOLD_DEGREES..=MAX_MANUAL_TURN_THRESHOLD_DEGREES)
            .contains(&manual.turn_threshold_degrees)
    {
        return Err(format!(
            "sync.manual.turnThresholdDegrees must be finite and between {MIN_MANUAL_TURN_THRESHOLD_DEGREES} and {MAX_MANUAL_TURN_THRESHOLD_DEGREES}"
        ));
    }

    let mut ids = Vec::with_capacity(manual.landmarks.len());
    let mut location_count = 0;
    for landmark in &manual.landmarks {
        let (id, video_second, activity_second) = match landmark {
            ProjectLandmark::Stop {
                id, video_second, ..
            }
            | ProjectLandmark::LeftTurn {
                id, video_second, ..
            }
            | ProjectLandmark::RightTurn {
                id, video_second, ..
            } => (id, *video_second, None),
            ProjectLandmark::Location {
                id,
                video_second,
                activity_second,
            } => {
                location_count += 1;
                (id, *video_second, *activity_second)
            }
        };
        if id.trim().is_empty() {
            return Err("sync.manual.landmark id must not be empty".into());
        }
        if ids.iter().any(|existing| *existing == id) {
            return Err(format!("sync.manual.landmark id is duplicated: {id}"));
        }
        ids.push(id);
        if !video_second.is_finite() || video_second < 0.0 {
            return Err(
                "sync.manual.landmark videoSecond must be a finite non-negative number".into(),
            );
        }
        if let Some(activity_second) = activity_second {
            if !activity_second.is_finite() {
                return Err(
                    "sync.manual.location landmark activitySecond must be finite or null".into(),
                );
            }
        }
    }
    if location_count > MAX_MANUAL_LOCATION_LANDMARKS {
        return Err(format!(
            "sync.manual.landmarks must contain at most {MAX_MANUAL_LOCATION_LANDMARKS} location landmark"
        ));
    }
    Ok(())
}

fn validate_editor(editor: &ProjectEditor) -> Result<(), String> {
    let globals = &editor.global_defaults;
    for (label, value) in [
        (
            "editor.globalDefaults.borderThickness",
            globals.border_thickness,
        ),
        (
            "editor.globalDefaults.shadowStrength",
            globals.shadow_strength,
        ),
        (
            "editor.globalDefaults.shadowDistance",
            globals.shadow_distance,
        ),
        ("editor.globalDefaults.fontSize", globals.font_size),
        ("editor.globalDefaults.opacity", globals.opacity),
        ("editor.globalDefaults.scale", globals.scale),
    ] {
        if !value.is_finite() {
            return Err(format!("{label} must be finite"));
        }
    }
    if globals.border_thickness < 0.0
        || globals.shadow_strength < 0.0
        || globals.shadow_distance < 0.0
        || globals.font_size <= 0.0
        || !(0.0..=1.0).contains(&globals.opacity)
        || globals.scale <= 0.0
    {
        return Err("Editor global defaults contain an out-of-range number".into());
    }
    for (label, value) in [
        ("borderColor", &globals.border_color),
        ("shadowColor", &globals.shadow_color),
        ("fontValues", &globals.font_values),
        ("fontText", &globals.font_text),
        ("colorValues", &globals.color_values),
        ("colorText", &globals.color_text),
        ("colorIcons", &globals.color_icons),
        ("colorUnits", &globals.color_units),
    ] {
        if value.trim().is_empty() {
            return Err(format!("editor.globalDefaults.{label} must not be empty"));
        }
    }

    editor
        .config
        .get("scene")
        .and_then(Value::as_object)
        .ok_or_else(|| "editor.config.scene must be an object".to_string())?;
    Ok(())
}

fn migrate_project(version: u32, value: Value) -> Result<ProjectDocument, String> {
    let mut legacy = match version {
        PROJECT_VERSION_V1 => {
            let project: ProjectDocumentV1 = serde_json::from_value(value)
                .map_err(|error| format!("Invalid version 1 project: {error}"))?;
            ProjectDocumentV2 {
                format: project.format,
                version: PROJECT_VERSION_V2,
                saved_at: project.saved_at,
                editor: project.editor,
                sources: project.sources,
                sync: ProjectSync {
                    video_offset_seconds: project.sync.video_offset_seconds,
                    video_timezone_mode: project.sync.video_timezone_mode,
                    manual: ProjectManualVideoSync {
                        landmarks: Vec::new(),
                        detected_location_second: None,
                        speed_threshold_kmh: DEFAULT_MANUAL_SPEED_THRESHOLD_KMH,
                        turn_threshold_degrees: DEFAULT_MANUAL_TURN_THRESHOLD_DEGREES,
                    },
                },
                render: project.render,
                timeline: project.timeline,
            }
        }
        PROJECT_VERSION_V2 => serde_json::from_value(value)
            .map_err(|error| format!("Invalid version 2 project: {error}"))?,
        PROJECT_VERSION => {
            return serde_json::from_value(value)
                .map_err(|error| format!("Invalid version 3 project: {error}"));
        }
        other => return Err(format!("Unsupported project version: {other}")),
    };
    let config = legacy
        .editor
        .config
        .as_object_mut()
        .ok_or_else(|| "editor.config must be an object".to_string())?;
    if config.contains_key("rasters") {
        return Err("Legacy project cannot contain rasters".into());
    }
    config.insert("rasters".into(), Value::Array(Vec::new()));
    Ok(ProjectDocument {
        format: legacy.format,
        version: PROJECT_VERSION,
        saved_at: legacy.saved_at,
        editor: legacy.editor,
        sources: legacy.sources,
        sync: legacy.sync,
        render: legacy.render,
        timeline: legacy.timeline,
        raster_assets: BTreeMap::new(),
    })
}

fn parse_project_header(input: &str) -> Result<(String, u32, Value), String> {
    let value: Value =
        serde_json::from_str(input).map_err(|error| format!("Invalid project JSON: {error}"))?;
    let object = value
        .as_object()
        .ok_or_else(|| "Project JSON root must be an object".to_string())?;
    let format = object
        .get("format")
        .and_then(Value::as_str)
        .ok_or_else(|| "Project format must be a string".to_string())?
        .to_string();
    let version = object
        .get("version")
        .and_then(Value::as_u64)
        .ok_or_else(|| "Project version must be an unsigned integer".to_string())?;
    let version = u32::try_from(version)
        .map_err(|_| "Project version is outside the supported range".to_string())?;
    Ok((format, version, value))
}

fn parse_project(input: &str) -> Result<ValidatedProject, String> {
    let (format, version, value) = parse_project_header(input)?;
    if format != PROJECT_FORMAT {
        return Err(format!("Unsupported project format: {format}"));
    }
    let mut project = migrate_project(version, value)?;
    let populated_rasters = validate_project(&project)?;
    let scene = project
        .editor
        .config
        .get_mut("scene")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| "editor.config.scene must be an object".to_string())?;
    scene.insert("fps".into(), Value::from(project.render.fps));
    scene.insert(
        "updateRate".into(),
        Value::from(project.render.widget_update_rate),
    );
    scene.remove("update_rate");
    Ok(ValidatedProject {
        document: project,
        populated_rasters,
    })
}

fn resolve_locator(project_path: &Path, locator: &PathLocator) -> Result<PathBuf, String> {
    validate_locator(locator)?;
    let path = match locator {
        PathLocator::Absolute(value) => PathBuf::from(value),
        PathLocator::ProjectRelative(value) => project_path
            .parent()
            .ok_or_else(|| "Project path has no parent directory".to_string())?
            .join(value),
    };
    Ok(path)
}

fn path_string(path: PathBuf) -> String {
    path.to_string_lossy().into_owned()
}

fn resolved_sources(
    project_path: &Path,
    project: &ProjectDocument,
) -> Result<ResolvedSources, String> {
    let activity_path = project
        .sources
        .activity
        .as_ref()
        .map(|source| resolve_locator(project_path, &source.path).map(path_string))
        .transpose()?;
    let video_path = project
        .sources
        .video
        .as_ref()
        .map(|source| resolve_locator(project_path, &source.path).map(path_string))
        .transpose()?;
    Ok(ResolvedSources {
        activity_path,
        video_path,
    })
}

struct ArchiveEntries {
    project: usize,
    rasters: HashMap<String, usize>,
}

fn inspect_archive(archive: &mut ZipArchive<File>) -> Result<ArchiveEntries, ProjectFileError> {
    let mut project = None;
    let mut names = HashSet::new();
    let mut rasters = HashMap::new();
    let mut total_uncompressed = 0u64;
    for index in 0..archive.len() {
        let entry = archive
            .by_index_raw(index)
            .map_err(|error| error.to_string())?;
        let name = entry.name();
        if !names.insert(name.to_string()) {
            return Err(format!("Duplicate project archive entry: {name}").into());
        }
        if entry.is_dir()
            || entry
                .unix_mode()
                .is_some_and(|mode| mode & 0o170000 != 0 && mode & 0o170000 != 0o100000)
        {
            return Err(format!("Project archive entry is not a regular file: {name}").into());
        }
        total_uncompressed = total_uncompressed.saturating_add(entry.size());
        if total_uncompressed > MAX_ARCHIVE_SIZE {
            return Err(ProjectFileError::ArchiveSize);
        }
        match name {
            PROJECT_JSON_ENTRY => {
                if entry.size() > MAX_PROJECT_JSON_SIZE {
                    return Err("project.json exceeds the 4 MiB size limit"
                        .to_string()
                        .into());
                }
                project = Some(index);
            }
            THUMBNAIL_ENTRY => {
                if entry.size() > MAX_THUMBNAIL_SIZE {
                    return Err("thumbnail.png exceeds the 2 MiB size limit"
                        .to_string()
                        .into());
                }
            }
            name if name
                .strip_prefix(RASTER_ASSET_NAMESPACE)
                .is_some_and(|file| {
                    !file.is_empty() && file != "." && file != ".." && !file.contains(['/', '\\'])
                }) =>
            {
                rasters.insert(name.to_string(), index);
            }
            name => return Err(format!("Unexpected project archive entry: {name}").into()),
        }
    }
    let project = project.ok_or_else(|| "Project archive is missing project.json".to_string())?;
    Ok(ArchiveEntries { project, rasters })
}

fn read_archive(path: &Path) -> Result<LoadedArchive, ProjectFileError> {
    let file = File::open(path).map_err(|error| error.to_string())?;
    if file.metadata().map_err(|error| error.to_string())?.len() > MAX_ARCHIVE_SIZE {
        return Err(ProjectFileError::ArchiveSize);
    }
    let mut archive =
        ZipArchive::new(file).map_err(|error| format!("Invalid project archive: {error}"))?;
    let entries = inspect_archive(&mut archive)?;
    let mut entry = archive
        .by_index(entries.project)
        .map_err(|error| error.to_string())?;
    let mut contents = String::with_capacity(entry.size() as usize);
    entry
        .read_to_string(&mut contents)
        .map_err(|error| format!("project.json is not valid UTF-8: {error}"))?;
    drop(entry);
    let ValidatedProject {
        document,
        populated_rasters,
    } = parse_project(&contents)?;
    let mapped_names: HashSet<_> = document.raster_assets.values().collect();
    for name in entries.rasters.keys() {
        if !mapped_names.contains(name) {
            return Err(format!("Unmapped raster archive entry: {name}").into());
        }
    }
    let rasters = load_raster_assets(
        &mut archive,
        &entries.rasters,
        &document.raster_assets,
        populated_rasters,
    );
    Ok(LoadedArchive {
        project: document,
        rasters,
    })
}

/// Names have passed document validation. Each populated widget gets exactly
/// one outcome, including widgets whose mapping or archive entry is missing.
fn load_raster_assets(
    archive: &mut ZipArchive<File>,
    entries: &HashMap<String, usize>,
    mappings: &BTreeMap<String, String>,
    populated: HashSet<String>,
) -> HashMap<String, Result<SelectedRaster, RasterError>> {
    populated
        .into_iter()
        .map(|id| {
            let result = mappings
                .get(&id)
                .ok_or(RasterError::MissingAsset)
                .and_then(|name| load_raster_asset(archive, entries, name));
            (id, result)
        })
        .collect()
}

fn load_raster_asset(
    archive: &mut ZipArchive<File>,
    entries: &HashMap<String, usize>,
    name: &str,
) -> Result<SelectedRaster, RasterError> {
    let index = entries.get(name).ok_or(RasterError::MissingAsset)?;
    let entry = archive
        .by_index(*index)
        .map_err(|_| RasterError::CorruptAsset)?;
    if entry.size() > MAX_ENCODED_BYTES {
        return Err(RasterError::EncodedSize);
    }
    let mut bytes = Vec::with_capacity(entry.size() as usize);
    entry
        .take(MAX_ENCODED_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| RasterError::CorruptAsset)?;
    let extension = name
        .rsplit_once('.')
        .expect("validated raster asset name")
        .1;
    load_embedded_raster(extension, bytes).map_err(|error| match error {
        RasterError::Decode | RasterError::UnsupportedType => RasterError::CorruptAsset,
        error => error,
    })
}

#[tauri::command]
pub(crate) fn read_project_file(
    resources: tauri::State<'_, RasterResources>,
    path: String,
) -> Result<ReadProjectResult, String> {
    read_project_file_sync(&resources, path).map_err(|error| error.command_error("invalid_archive"))
}

fn read_project_file_sync(
    resources: &RasterResources,
    path: String,
) -> Result<ReadProjectResult, ProjectFileError> {
    let project_path = PathBuf::from(path);
    let LoadedArchive { project, rasters } = read_archive(&project_path)?;
    let resolved_sources = resolved_sources(&project_path, &project)?;
    let mut raster_load_results = HashMap::with_capacity(rasters.len());
    for (id, image) in rasters {
        let result = match image {
            Ok(image) => RasterLoadResult::Ready {
                resource_id: resources.insert(image),
            },
            Err(error_code) => RasterLoadResult::Error { error_code },
        };
        raster_load_results.insert(id, result);
    }
    Ok(ReadProjectResult {
        project,
        resolved_sources,
        raster_load_results,
    })
}

fn write_archive(
    path: &Path,
    project_json: &str,
    thumbnail: Option<&[u8]>,
    assets: &BTreeMap<String, RasterAsset>,
) -> Result<(), ProjectFileError> {
    let uncompressed_size = project_json.len() as u64
        + thumbnail.map_or(0, |bytes| bytes.len() as u64)
        + assets
            .values()
            .map(|asset| asset.image.encoded_bytes().len() as u64)
            .sum::<u64>();
    if uncompressed_size > MAX_ARCHIVE_SIZE {
        return Err(ProjectFileError::ArchiveSize);
    }
    let file = File::create(path).map_err(|error| error.to_string())?;
    let mut writer = ZipWriter::new(file);
    write_archive_entry(&mut writer, PROJECT_JSON_ENTRY, project_json.as_bytes())?;
    if let Some(thumbnail) = thumbnail {
        write_archive_entry(&mut writer, THUMBNAIL_ENTRY, thumbnail)?;
    }
    for asset in assets.values() {
        write_archive_entry(&mut writer, &asset.name, asset.image.encoded_bytes())?;
    }
    let file = writer.finish().map_err(|error| error.to_string())?;
    file.sync_all().map_err(|error| error.to_string())?;
    if file.metadata().map_err(|error| error.to_string())?.len() > MAX_ARCHIVE_SIZE {
        return Err(ProjectFileError::ArchiveSize);
    }
    Ok(())
}

fn write_archive_entry(
    writer: &mut ZipWriter<File>,
    name: &str,
    contents: &[u8],
) -> Result<(), ProjectFileError> {
    writer
        .start_file(
            name,
            SimpleFileOptions::default().compression_method(CompressionMethod::Deflated),
        )
        .map_err(|error| error.to_string())?;
    writer
        .write_all(contents)
        .map_err(|error| error.to_string().into())
}

fn generate_thumbnail(ffmpeg: &Path, video_path: &Path) -> Option<Vec<u8>> {
    let mut command = Command::new(ffmpeg);
    ovrley_core::encode::ffmpeg::binary::configure_ffmpeg_command(&mut command);
    let output = command
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-i")
        .arg(video_path)
        .arg("-frames:v")
        .arg("1")
        .arg("-vf")
        .arg(THUMBNAIL_FILTER)
        .arg("-f")
        .arg("image2pipe")
        .arg("-vcodec")
        .arg("png")
        .arg("pipe:1")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success()
        || output.stdout.is_empty()
        || output.stdout.len() as u64 > MAX_THUMBNAIL_SIZE
    {
        return None;
    }
    Some(output.stdout)
}

#[cfg(windows)]
fn atomic_replace(source: &Path, target: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;

    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;

    #[link(name = "Kernel32")]
    extern "system" {
        fn MoveFileExW(
            existing_file_name: *const u16,
            new_file_name: *const u16,
            flags: u32,
        ) -> i32;
    }

    let source_wide: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let target_wide: Vec<u16> = target.as_os_str().encode_wide().chain(Some(0)).collect();
    let result = unsafe {
        MoveFileExW(
            source_wide.as_ptr(),
            target_wide.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(())
}

#[cfg(not(windows))]
fn atomic_replace(source: &Path, target: &Path) -> Result<(), String> {
    fs::rename(source, target).map_err(|error| error.to_string())
}

/// Owns the temporary archive until the atomic replacement succeeds. Every
/// error and unwinding path releases it without changing the destination.
struct PendingArchive {
    path: Option<PathBuf>,
}

impl PendingArchive {
    fn new(directory: &Path) -> Self {
        Self {
            path: Some(directory.join(format!(".ovrley-project-{}.tmp", Uuid::new_v4()))),
        }
    }

    fn path(&self) -> &Path {
        self.path.as_deref().expect("uncommitted archive")
    }

    fn commit(mut self, target: &Path) -> Result<(), String> {
        atomic_replace(self.path(), target)?;
        self.path.take();
        Ok(())
    }
}

impl Drop for PendingArchive {
    fn drop(&mut self) {
        if let Some(path) = &self.path {
            if let Err(error) = fs::remove_file(path) {
                if error.kind() != std::io::ErrorKind::NotFound {
                    log::warn!(
                        "Failed to remove temporary project archive {}: {error}",
                        path.display()
                    );
                }
            }
        }
    }
}

/// The save operation owns the reachable images and their archive names.
/// Both the persisted mapping and ZIP entries consume these same names.
fn prepare_raster_assets(
    populated: &HashSet<String>,
    images: HashMap<String, Arc<SelectedRaster>>,
) -> Result<BTreeMap<String, RasterAsset>, ProjectFileError> {
    if populated.len() != images.len() || images.keys().any(|id| !populated.contains(id)) {
        return Err(ProjectFileError::RasterUnavailable(
            "Every populated raster requires its owned image resource before saving".into(),
        ));
    }
    Ok(images
        .into_iter()
        .map(|(id, image)| {
            let name = format!("{RASTER_ASSET_NAMESPACE}{id}.{}", image.extension());
            (id, RasterAsset { name, image })
        })
        .collect())
}

fn write_project_file_sync(
    path: String,
    project_json: String,
    ffmpeg: Option<&Path>,
    images: HashMap<String, Arc<SelectedRaster>>,
) -> Result<String, ProjectFileError> {
    let ValidatedProject {
        document: mut project,
        populated_rasters,
    } = parse_project(&project_json)?;
    let assets = prepare_raster_assets(&populated_rasters, images)?;
    project.raster_assets = assets
        .iter()
        .map(|(id, asset)| (id.clone(), asset.name.clone()))
        .collect();
    let canonical_json = serde_json::to_string(&project)
        .map_err(|error| format!("Failed to serialize canonical project: {error}"))?;
    if canonical_json.len() as u64 > MAX_PROJECT_JSON_SIZE {
        return Err("project.json exceeds the 4 MiB size limit"
            .to_string()
            .into());
    }
    let target = PathBuf::from(&path);
    let parent = target
        .parent()
        .ok_or_else(|| "Project target has no parent directory".to_string())?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let thumbnail = project
        .sources
        .video
        .as_ref()
        .and_then(|source| resolve_locator(&target, &source.path).ok())
        .and_then(|video_path| ffmpeg.and_then(|ffmpeg| generate_thumbnail(ffmpeg, &video_path)));
    let temporary = PendingArchive::new(parent);
    write_archive(
        temporary.path(),
        &canonical_json,
        thumbnail.as_deref(),
        &assets,
    )?;
    temporary.commit(&target)?;
    Ok(path)
}

#[tauri::command]
pub(crate) async fn write_project_file(
    app: tauri::AppHandle,
    resources: tauri::State<'_, RasterResources>,
    path: String,
    project_json: String,
    raster_resource_ids: HashMap<String, String>,
) -> Result<String, String> {
    let images = resources
        .owned_assets(raster_resource_ids)
        .map_err(|error| format!("[project_error:raster_unavailable] {error}"))?;
    let ffmpeg = crate::runtime_paths::app_paths(&app)
        .ok()
        .and_then(|paths| {
            ovrley_core::encode::ffmpeg::binary::resolve_ffmpeg_binary(&paths.repo_root).ok()
        });
    tauri::async_runtime::spawn_blocking(move || {
        write_project_file_sync(path, project_json, ffmpeg.as_deref(), images)
    })
    .await
    .map_err(|error| format!("[project_error:save_failed] Project save task failed: {error}"))?
    .map_err(|error| error.command_error("save_failed"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ovrley_core::raster::RasterResourceResolver;

    fn valid_project_json() -> String {
        let config = include_str!("../../templates/acid-titanium.json");
        let fixture: Value = serde_json::from_str(config).unwrap();
        let mut config = fixture["config"].clone();
        config["rasters"] = serde_json::json!([]);
        serde_json::json!({
            "format": PROJECT_FORMAT,
            "version": PROJECT_VERSION,
            "savedAt": "2026-08-27T12:00:00.000Z",
            "editor": {
                "config": config,
                "globalDefaults": fixture["settings"]["globalDefaults"]
            },
            "sources": {
                "activity": { "path": { "kind": "project-relative", "value": "media/session.fit" } },
                "video": null
            },
            "sync": {
                "videoOffsetSeconds": 0.0,
                "videoTimezoneMode": null,
                "manual": {
                    "landmarks": [],
                    "detectedLocationSecond": null,
                    "speedThresholdKmh": 5.0,
                    "turnThresholdDegrees": 90.0
                }
            },
            "render": {
                "fps": 30.0, "widgetUpdateRate": 1, "exportMode": "transparent", "codec": "prores_ks",
                "bitrateMbps": null, "range": { "type": "all", "from": 0.0, "to": 0.0 }
            },
            "timeline": { "playheadSecond": 0.0, "viewStart": 0.0, "viewEnd": 73.0 },
            "rasterAssets": {}
        }).to_string()
    }

    fn valid_v1_project_json() -> String {
        let mut project: Value = serde_json::from_str(&valid_v2_project_json()).unwrap();
        project["version"] = Value::from(PROJECT_VERSION_V1);
        project["sync"].as_object_mut().unwrap().remove("manual");
        project.to_string()
    }

    fn valid_v2_project_json() -> String {
        let mut project: Value = serde_json::from_str(&valid_project_json()).unwrap();
        project["version"] = Value::from(PROJECT_VERSION_V2);
        project["editor"]["config"]
            .as_object_mut()
            .unwrap()
            .remove("rasters");
        project.as_object_mut().unwrap().remove("rasterAssets");
        project.to_string()
    }

    fn bmp(pixel: [u8; 3]) -> Vec<u8> {
        // A 1x1, 24-bit BMP with a padded BGR pixel at byte 54.
        let mut bytes = BASE64_STANDARD
            .decode(
                "Qk06AAAAAAAAADYAAAAoAAAAAQAAAAEAAAABABgAAAAAAAQAAAAAAAAAAAAAAAAAAAAAAAAAAAD/AA==",
            )
            .unwrap();
        bytes[54..57].copy_from_slice(&pixel);
        bytes
    }

    fn raster_project(rasters: Value) -> String {
        let mut project: Value = serde_json::from_str(&valid_project_json()).unwrap();
        project["editor"]["config"]["rasters"] = rasters;
        project.to_string()
    }

    fn raster(id: &str, path: &Path) -> Value {
        serde_json::json!({
            "id": id, "x": 0, "y": 0, "width": 1, "height": 1,
            "rotation": 0, "opacity": 1, "path": path_string(path.to_path_buf())
        })
    }

    #[test]
    fn embedded_rasters_survive_source_loss_and_follow_reachable_widgets() {
        let directory =
            std::env::temp_dir().join(format!("ovrley-raster-project-{}", Uuid::new_v4()));
        fs::create_dir_all(&directory).unwrap();
        let source = directory.join("source.bmp");
        let path = directory.join("project.oly");
        let original = bmp([0, 0, 255]);
        fs::write(&source, &original).unwrap();
        let first = Arc::new(ovrley_core::raster::load_selected_raster(&source).unwrap());
        let expected_preview = BASE64_STANDARD.encode(first.preview_png());
        let images = HashMap::from([("one".into(), Arc::clone(&first)), ("two".into(), first)]);
        let project_json = raster_project(serde_json::json!([
            raster("one", &source),
            raster("two", &source)
        ]));
        write_project_file_sync(path_string(path.clone()), project_json, None, images).unwrap();
        fs::remove_file(&source).unwrap();

        let resources = RasterResources::default();
        let loaded = read_project_file_sync(&resources, path_string(path.clone())).unwrap();
        for id in ["one", "two"] {
            let RasterLoadResult::Ready { resource_id } = &loaded.raster_load_results[id] else {
                panic!("expected ready image for {id}");
            };
            assert_eq!(
                resources.resolve(resource_id).unwrap().encoded_bytes(),
                original
            );
            assert_eq!(
                resources.preview_png_base64(resource_id).unwrap(),
                expected_preview
            );
        }

        let replacement_bytes = bmp([0, 255, 0]);
        let replacement = Arc::new(load_embedded_raster("bmp", replacement_bytes.clone()).unwrap());
        let images = HashMap::from([("one".into(), replacement)]);
        let project_json = raster_project(serde_json::json!([raster("one", &source)]));
        write_project_file_sync(path_string(path.clone()), project_json, None, images).unwrap();
        let LoadedArchive {
            project,
            rasters: images,
        } = read_archive(&path).unwrap();
        assert_eq!(project.raster_assets.len(), 1);
        assert_eq!(
            images["one"].as_ref().unwrap().encoded_bytes(),
            replacement_bytes
        );
        let archive = ZipArchive::new(File::open(&path).unwrap()).unwrap();
        assert_eq!(archive.len(), 2);
        drop(archive);
        let project_json = serde_json::to_string(&project).unwrap();
        write_archive(&path, &project_json, None, &BTreeMap::new()).unwrap();
        let loaded = read_project_file_sync(&resources, path_string(path)).unwrap();
        assert!(matches!(
            loaded.raster_load_results["one"],
            RasterLoadResult::Error {
                error_code: RasterError::MissingAsset
            }
        ));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn archive_limit_does_not_replace_an_existing_project() {
        let directory =
            std::env::temp_dir().join(format!("ovrley-raster-limit-{}", Uuid::new_v4()));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("project.oly");
        write_archive(&path, &valid_project_json(), None, &BTreeMap::new()).unwrap();
        let previous = fs::read(&path).unwrap();
        let mut bytes = bmp([0, 0, 255]);
        bytes.resize(MAX_ENCODED_BYTES as usize, 0);
        let image = Arc::new(load_embedded_raster("bmp", bytes).unwrap());
        let mut rasters = Vec::new();
        let mut images = HashMap::new();
        for index in 0..8 {
            let id = format!("image-{index}");
            rasters.push(raster(&id, &directory.join("gone.bmp")));
            images.insert(id, Arc::clone(&image));
        }
        let project_json = raster_project(Value::Array(rasters));
        let error = write_project_file_sync(path_string(path.clone()), project_json, None, images)
            .unwrap_err();
        assert!(matches!(error, ProjectFileError::ArchiveSize));
        assert_eq!(fs::read(&path).unwrap(), previous);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn project_archive_round_trip_and_path_resolution() {
        let directory =
            std::env::temp_dir().join(format!("ovrley-project-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("Race.oly");
        write_project_file_sync(
            path_string(path.clone()),
            valid_project_json(),
            None,
            HashMap::new(),
        )
        .unwrap();
        let result =
            read_project_file_sync(&RasterResources::default(), path_string(path)).unwrap();
        assert_eq!(result.project.version, PROJECT_VERSION);
        assert_eq!(
            result.resolved_sources.activity_path,
            Some(path_string(directory.join("media/session.fit")))
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn v2_manual_sync_round_trip_preserves_landmarks_location_and_thresholds() {
        let mut project: Value = serde_json::from_str(&valid_project_json()).unwrap();
        project["sources"]["video"] = serde_json::json!({
            "path": { "kind": "project-relative", "value": "media/video.mp4" }
        });
        project["sync"]["manual"] = serde_json::json!({
            "landmarks": [
                { "id": "stop-1", "type": "stop", "videoSecond": 4.0 },
                { "id": "left-1", "type": "leftTurn", "videoSecond": 8.0 },
                { "id": "right-1", "type": "rightTurn", "videoSecond": 12.0 },
                { "id": "location-1", "type": "location", "videoSecond": 16.0, "activitySecond": null }
            ],
            "detectedLocationSecond": 42.5,
            "speedThresholdKmh": 7.0,
            "turnThresholdDegrees": 120.0
        });

        let parsed = parse_project(&project.to_string()).unwrap().document;
        let serialized = serde_json::to_value(parsed).unwrap();
        assert_eq!(serialized["sync"]["manual"], project["sync"]["manual"]);
    }

    #[test]
    fn older_v2_manual_sync_without_detected_location_loads_as_none() {
        let mut project: Value = serde_json::from_str(&valid_project_json()).unwrap();
        project["sync"]["manual"]
            .as_object_mut()
            .unwrap()
            .remove("detectedLocationSecond");

        let parsed = parse_project(&project.to_string()).unwrap().document;
        assert_eq!(parsed.sync.manual.detected_location_second, None);
    }

    #[test]
    fn rejects_invalid_or_unowned_detected_location() {
        let mut project: Value = serde_json::from_str(&valid_project_json()).unwrap();
        project["sync"]["manual"]["detectedLocationSecond"] = Value::from(-1.0);
        assert!(parse_project(&project.to_string()).is_err());

        project["sync"]["manual"]["detectedLocationSecond"] = Value::from(42.5);
        project["sources"]["activity"] = Value::Null;
        assert!(parse_project(&project.to_string()).is_err());
    }

    #[test]
    fn project_summary_includes_archived_thumbnail() {
        let directory =
            std::env::temp_dir().join(format!("ovrley-project-thumbnail-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("Race.oly");
        let thumbnail = b"\x89PNG\r\n\x1a\nthumbnail";
        write_archive(
            &path,
            &valid_project_json(),
            Some(thumbnail),
            &BTreeMap::new(),
        )
        .unwrap();

        let projects = list_project_files(path_string(directory.clone())).unwrap();

        assert_eq!(projects.len(), 1);
        assert_eq!(
            projects[0].thumbnail_data_url.as_deref(),
            Some(
                format!(
                    "data:image/png;base64,{}",
                    BASE64_STANDARD.encode(thumbnail)
                )
                .as_str()
            )
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn thumbnail_failure_does_not_block_project_save() {
        let directory =
            std::env::temp_dir().join(format!("ovrley-project-save-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("Race.oly");
        let mut project: Value = serde_json::from_str(&valid_project_json()).unwrap();
        project["sources"]["video"] = serde_json::json!({
            "path": { "kind": "project-relative", "value": "missing.mp4" }
        });
        let missing_ffmpeg = directory.join("missing-ffmpeg-binary");

        write_project_file_sync(
            path_string(path.clone()),
            project.to_string(),
            Some(&missing_ffmpeg),
            HashMap::new(),
        )
        .unwrap();

        assert!(
            read_project_file_sync(&RasterResources::default(), path_string(path.clone())).is_ok()
        );
        assert!(read_thumbnail_data_url(&path).is_none());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn rejects_invalid_contracts() {
        for invalid in [
            "not json".to_string(),
            valid_project_json().replace(PROJECT_FORMAT, "wrong-project"),
            valid_project_json().replace("\"version\":3", "\"version\":99"),
            valid_project_json().replace("\"fps\":30.0", "\"fps\":0.0"),
        ] {
            assert!(parse_project(&invalid).is_err());
        }
    }

    #[test]
    fn migrates_v1_and_v2_to_canonical_v3_without_embedded_rasters() {
        let parsed = parse_project(&valid_v1_project_json()).unwrap().document;
        assert_eq!(parsed.version, PROJECT_VERSION);
        assert!(parsed.sync.manual.landmarks.is_empty());
        assert_eq!(parsed.sync.manual.speed_threshold_kmh, 5.0);
        assert_eq!(parsed.sync.manual.turn_threshold_degrees, 80.0);
        let migrated = parse_project(&valid_v2_project_json()).unwrap().document;
        assert!(migrated.editor.config["rasters"]
            .as_array()
            .unwrap()
            .is_empty());
        assert!(migrated.raster_assets.is_empty());
        assert_eq!(migrated.sync.manual.speed_threshold_kmh, 5.0);
        let mut malformed: Value = serde_json::from_str(&valid_v2_project_json()).unwrap();
        malformed["editor"]["config"]["rasters"] = serde_json::json!([]);
        assert!(parse_project(&malformed.to_string()).is_err());
    }

    #[test]
    fn saving_a_v1_document_writes_canonical_v3() {
        let directory =
            std::env::temp_dir().join(format!("ovrley-project-v1-save-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("Legacy.oly");
        write_project_file_sync(
            path_string(path.clone()),
            valid_v1_project_json(),
            None,
            HashMap::new(),
        )
        .unwrap();
        assert_eq!(
            read_project_file_sync(&RasterResources::default(), path_string(path))
                .unwrap()
                .project
                .version,
            PROJECT_VERSION
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn rejects_malformed_v2_manual_sync_contracts() {
        let mut generic_turn: Value = serde_json::from_str(&valid_project_json()).unwrap();
        generic_turn["sync"]["manual"]["landmarks"] = serde_json::json!([
            { "id": "turn", "type": "turn", "videoSecond": 4.0 }
        ]);

        let mut missing_location_activity: Value =
            serde_json::from_str(&valid_project_json()).unwrap();
        missing_location_activity["sync"]["manual"]["landmarks"] = serde_json::json!([
            { "id": "location", "type": "location", "videoSecond": 4.0 }
        ]);

        let mut duplicate_ids: Value = serde_json::from_str(&valid_project_json()).unwrap();
        duplicate_ids["sync"]["manual"]["landmarks"] = serde_json::json!([
            { "id": "same", "type": "stop", "videoSecond": 1.0 },
            { "id": "same", "type": "leftTurn", "videoSecond": 2.0 }
        ]);

        let mut landmark_without_video: Value =
            serde_json::from_str(&valid_project_json()).unwrap();
        landmark_without_video["sources"]["video"] = Value::Null;
        landmark_without_video["sync"]["manual"]["landmarks"] = serde_json::json!([
            { "id": "stop", "type": "stop", "videoSecond": 1.0 }
        ]);

        for (label, invalid) in [
            ("generic turn", generic_turn),
            ("missing location activitySecond", missing_location_activity),
            ("duplicate landmark ids", duplicate_ids),
            ("landmark without video source", landmark_without_video),
        ] {
            assert!(
                parse_project(&invalid.to_string()).is_err(),
                "accepted invalid contract: {label}"
            );
        }
    }

    #[test]
    fn resolves_absolute_locator_without_rebasing() {
        let absolute = if cfg!(windows) {
            r"D:\media\ride.fit"
        } else {
            "/media/ride.fit"
        };
        let project = Path::new(if cfg!(windows) {
            r"C:\events\Race.oly"
        } else {
            "/events/Race.oly"
        });
        let resolved = resolve_locator(project, &PathLocator::Absolute(absolute.into())).unwrap();
        assert_eq!(resolved, PathBuf::from(absolute));
    }

    #[test]
    fn rejects_a_template_reference_in_the_project_contract() {
        let mut project: Value = serde_json::from_str(&valid_project_json()).unwrap();
        project["template"] = serde_json::json!({ "source": null });

        assert!(parse_project(&project.to_string()).is_err());
    }

    #[test]
    fn rejects_malformed_editor_widget_state() {
        let mut project: Value = serde_json::from_str(&valid_project_json()).unwrap();
        project["editor"]["globalDefaults"]["opacity"] = Value::from(2.0);

        assert!(parse_project(&project.to_string()).is_err());
    }

    #[test]
    fn legacy_widget_config_reaches_frontend_normalization() {
        let mut project: Value = serde_json::from_str(&valid_project_json()).unwrap();
        project["editor"]["config"]["values"][0]
            .as_object_mut()
            .unwrap()
            .remove("content_alignment");

        assert!(parse_project(&project.to_string()).is_ok());
    }

    #[test]
    fn normalizes_editor_render_mirrors_from_project_render_settings() {
        let mut project: Value = serde_json::from_str(&valid_project_json()).unwrap();
        project["editor"]["config"]["scene"]["fps"] = Value::from(60.0);
        project["editor"]["config"]["scene"]["updateRate"] = Value::from(4.0);

        let parsed = parse_project(&project.to_string()).unwrap().document;
        assert_eq!(parsed.editor.config["scene"]["fps"], Value::from(30.0));
        assert_eq!(parsed.editor.config["scene"]["updateRate"], Value::from(1));
    }

    #[test]
    fn rejects_malformed_saved_timestamp() {
        let mut project: Value = serde_json::from_str(&valid_project_json()).unwrap();
        project["savedAt"] = Value::from("notTtimestamp");

        assert!(parse_project(&project.to_string()).is_err());
    }
}
