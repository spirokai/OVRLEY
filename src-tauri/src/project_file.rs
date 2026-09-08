//! Strict `.oly` project archive boundary.
//!
//! The project envelope is validated here before frontend orchestration sees it.
//! Widget config normalization belongs to the frontend project-load seam, while
//! archive and platform-path details do not leak into application state.

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use chrono::DateTime;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use tauri::Manager;
use uuid::Uuid;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

const PROJECT_FORMAT: &str = "ovrley-project";
const PROJECT_VERSION: u32 = 1;
const PROJECT_JSON_ENTRY: &str = "project.json";
const THUMBNAIL_ENTRY: &str = "thumbnail.png";
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

fn validate_project(project: &ProjectDocument) -> Result<(), String> {
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
    validate_editor(&project.editor)?;
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

fn parse_project(input: &str) -> Result<ProjectDocument, String> {
    let mut project: ProjectDocument =
        serde_json::from_str(input).map_err(|error| format!("Invalid project JSON: {error}"))?;
    validate_project(&project)?;
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
    Ok(project)
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

fn read_archive(path: &Path) -> Result<ProjectDocument, String> {
    let metadata = fs::metadata(path).map_err(|error| error.to_string())?;
    if metadata.len() > MAX_ARCHIVE_SIZE {
        return Err("Project archive exceeds the 40 MiB size limit".into());
    }
    let file = File::open(path).map_err(|error| error.to_string())?;
    let mut archive =
        ZipArchive::new(file).map_err(|error| format!("Invalid project archive: {error}"))?;
    let mut project_index = None;
    let mut thumbnail_seen = false;
    for index in 0..archive.len() {
        let entry = archive
            .by_index_raw(index)
            .map_err(|error| error.to_string())?;
        match entry.name() {
            PROJECT_JSON_ENTRY => {
                if project_index.replace(index).is_some() {
                    return Err("Project archive contains duplicate project.json entries".into());
                }
                if entry.size() > MAX_PROJECT_JSON_SIZE {
                    return Err("project.json exceeds the 4 MiB size limit".into());
                }
            }
            THUMBNAIL_ENTRY => {
                if thumbnail_seen {
                    return Err("Project archive contains duplicate thumbnail.png entries".into());
                }
                thumbnail_seen = true;
                if entry.size() > MAX_THUMBNAIL_SIZE {
                    return Err("thumbnail.png exceeds the 2 MiB size limit".into());
                }
            }
            name => return Err(format!("Unexpected project archive entry: {name}")),
        }
    }
    let index =
        project_index.ok_or_else(|| "Project archive is missing project.json".to_string())?;
    let mut entry = archive.by_index(index).map_err(|error| error.to_string())?;
    let mut contents = String::with_capacity(entry.size() as usize);
    entry
        .read_to_string(&mut contents)
        .map_err(|error| format!("project.json is not valid UTF-8: {error}"))?;
    parse_project(&contents)
}

#[tauri::command]
pub(crate) fn read_project_file(path: String) -> Result<ReadProjectResult, String> {
    let project_path = PathBuf::from(path);
    let project = read_archive(&project_path)?;
    let resolved_sources = resolved_sources(&project_path, &project)?;
    Ok(ReadProjectResult {
        project,
        resolved_sources,
    })
}

fn write_archive(path: &Path, project_json: &str, thumbnail: Option<&[u8]>) -> Result<(), String> {
    let file = File::create(path).map_err(|error| error.to_string())?;
    let mut writer = ZipWriter::new(file);
    writer
        .start_file(
            PROJECT_JSON_ENTRY,
            SimpleFileOptions::default().compression_method(CompressionMethod::Deflated),
        )
        .map_err(|error| error.to_string())?;
    writer
        .write_all(project_json.as_bytes())
        .map_err(|error| error.to_string())?;
    if let Some(thumbnail) = thumbnail {
        writer
            .start_file(
                THUMBNAIL_ENTRY,
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated),
            )
            .map_err(|error| error.to_string())?;
        writer
            .write_all(thumbnail)
            .map_err(|error| error.to_string())?;
    }
    let file = writer.finish().map_err(|error| error.to_string())?;
    file.sync_all().map_err(|error| error.to_string())?;
    Ok(())
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

fn write_project_file_sync(
    path: String,
    project_json: String,
    ffmpeg: Option<&Path>,
) -> Result<String, String> {
    let project = parse_project(&project_json)?;
    if project_json.len() as u64 > MAX_PROJECT_JSON_SIZE {
        return Err("project.json exceeds the 4 MiB size limit".into());
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
    let temporary = parent.join(format!(".ovrley-project-{}.tmp", Uuid::new_v4()));
    if let Err(error) = write_archive(&temporary, &project_json, thumbnail.as_deref()) {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    atomic_replace(&temporary, &target).map_err(|error| {
        let _ = fs::remove_file(&temporary);
        error
    })?;
    Ok(path)
}

#[tauri::command]
pub(crate) async fn write_project_file(
    app: tauri::AppHandle,
    path: String,
    project_json: String,
) -> Result<String, String> {
    let ffmpeg = crate::runtime_paths::app_paths(&app)
        .ok()
        .and_then(|paths| {
            ovrley_core::encode::ffmpeg::binary::resolve_ffmpeg_binary(&paths.repo_root).ok()
        });
    tauri::async_runtime::spawn_blocking(move || {
        write_project_file_sync(path, project_json, ffmpeg.as_deref())
    })
    .await
    .map_err(|error| format!("Project save task failed: {error}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_project_json() -> String {
        let config = include_str!("../../templates/acid-titanium.json");
        let fixture: Value = serde_json::from_str(config).unwrap();
        serde_json::json!({
            "format": PROJECT_FORMAT,
            "version": PROJECT_VERSION,
            "savedAt": "2026-08-27T12:00:00.000Z",
            "editor": {
                "config": fixture["config"],
                "globalDefaults": fixture["settings"]["globalDefaults"]
            },
            "sources": {
                "activity": { "path": { "kind": "project-relative", "value": "media/session.fit" } },
                "video": null
            },
            "sync": { "videoOffsetSeconds": 0.0, "videoTimezoneMode": null },
            "render": {
                "fps": 30.0, "widgetUpdateRate": 1, "exportMode": "transparent", "codec": "prores_ks",
                "bitrateMbps": null, "range": { "type": "all", "from": 0.0, "to": 0.0 }
            },
            "timeline": { "playheadSecond": 0.0, "viewStart": 0.0, "viewEnd": 73.0 }
        }).to_string()
    }

    #[test]
    fn project_archive_round_trip_and_path_resolution() {
        let directory =
            std::env::temp_dir().join(format!("ovrley-project-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("Race.oly");
        write_project_file_sync(path_string(path.clone()), valid_project_json(), None).unwrap();
        let result = read_project_file(path_string(path)).unwrap();
        assert_eq!(result.project.version, PROJECT_VERSION);
        assert_eq!(
            result.resolved_sources.activity_path,
            Some(path_string(directory.join("media/session.fit")))
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn project_summary_includes_archived_thumbnail() {
        let directory =
            std::env::temp_dir().join(format!("ovrley-project-thumbnail-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("Race.oly");
        let thumbnail = b"\x89PNG\r\n\x1a\nthumbnail";
        write_archive(&path, &valid_project_json(), Some(thumbnail)).unwrap();

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
        )
        .unwrap();

        assert!(read_project_file(path_string(path.clone())).is_ok());
        assert!(read_thumbnail_data_url(&path).is_none());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn rejects_invalid_contracts() {
        for invalid in [
            "not json".to_string(),
            valid_project_json().replace(PROJECT_FORMAT, "wrong-project"),
            valid_project_json().replace("\"version\":1", "\"version\":99"),
            valid_project_json().replace("\"fps\":30.0", "\"fps\":0.0"),
        ] {
            assert!(parse_project(&invalid).is_err());
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

        let parsed = parse_project(&project.to_string()).unwrap();
        assert_eq!(parsed.editor.config["scene"]["fps"], Value::from(30.0));
        assert_eq!(
            parsed.editor.config["scene"]["updateRate"],
            Value::from(1.0)
        );
    }

    #[test]
    fn rejects_malformed_saved_timestamp() {
        let mut project: Value = serde_json::from_str(&valid_project_json()).unwrap();
        project["savedAt"] = Value::from("notTtimestamp");

        assert!(parse_project(&project.to_string()).is_err());
    }
}
