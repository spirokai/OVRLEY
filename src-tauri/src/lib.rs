#[cfg_attr(mobile, tauri::mobile_entry_point)]
use ovrley_core::commands::{self, AppPaths};
use ovrley_core::encode::video::RenderController;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

struct BackendState {
    render_controller: RenderController,
}

fn source_repo_root() -> PathBuf {
    let mut root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    root.pop();
    root
}

fn app_paths(app: &AppHandle) -> Result<AppPaths, String> {
    let repo_root = source_repo_root();
    let resource_root = if cfg!(debug_assertions) {
        repo_root.clone()
    } else {
        app.path().resource_dir().map_err(|e| e.to_string())?
    };
    let mut paths = AppPaths::from_resource_root(repo_root, resource_root);
    paths.user_templates_dir = app
        .path()
        .document_dir()
        .map_err(|e| e.to_string())?
        .join("OVRLEY");
    paths.ensure_dirs()?;
    Ok(paths)
}

#[tauri::command]
async fn backend_health(app: AppHandle) -> Result<String, String> {
    let response = commands::backend_health(&app_paths(&app)?);
    serde_json::to_string(&response).map_err(|error| error.to_string())
}

#[tauri::command]
async fn backend_current_os() -> Result<String, String> {
    let response = commands::backend_current_os();
    serde_json::to_string(&response).map_err(|error| error.to_string())
}

#[tauri::command]
async fn backend_list_system_fonts() -> Result<String, String> {
    let response = commands::backend_list_system_fonts();
    serde_json::to_string(&response).map_err(|error| error.to_string())
}

#[tauri::command]
async fn backend_demo(
    app: AppHandle,
    config_json: String,
    parsed_activity_json: String,
    second: u32,
) -> Result<String, String> {
    let response = commands::backend_demo(
        &app_paths(&app)?,
        &config_json,
        &parsed_activity_json,
        second,
    )?;
    serde_json::to_string(&response).map_err(|error| error.to_string())
}

#[tauri::command]
async fn backend_render(
    app: AppHandle,
    state: tauri::State<'_, BackendState>,
    config_json: String,
    parsed_activity_json: String,
) -> Result<String, String> {
    let response = commands::backend_render(
        &app_paths(&app)?,
        &state.render_controller,
        &config_json,
        &parsed_activity_json,
    )?;
    serde_json::to_string(&response).map_err(|error| error.to_string())
}

#[tauri::command]
async fn backend_progress(state: tauri::State<'_, BackendState>) -> Result<String, String> {
    let response = commands::backend_progress(&state.render_controller);
    serde_json::to_string(&response).map_err(|error| error.to_string())
}

#[tauri::command]
async fn backend_open_downloads(app: AppHandle) -> Result<String, String> {
    let response = commands::backend_open_downloads(&app_paths(&app)?)?;
    serde_json::to_string(&response).map_err(|error| error.to_string())
}

#[tauri::command]
async fn backend_open_video(app: AppHandle, filename: String) -> Result<String, String> {
    let response = commands::backend_open_video(&app_paths(&app)?, &filename)?;
    serde_json::to_string(&response).map_err(|error| error.to_string())
}

#[tauri::command]
async fn backend_list_templates(app: AppHandle) -> Result<String, String> {
    let response = commands::backend_list_templates(&app_paths(&app)?)?;
    serde_json::to_string(&response).map_err(|error| error.to_string())
}

#[tauri::command]
async fn backend_get_template(app: AppHandle, filename: String) -> Result<String, String> {
    commands::backend_get_template(&app_paths(&app)?, &filename)
}

#[tauri::command]
async fn backend_cancel(state: tauri::State<'_, BackendState>) -> Result<String, String> {
    let response = commands::backend_cancel(&state.render_controller);
    serde_json::to_string(&response).map_err(|error| error.to_string())
}

#[tauri::command]
async fn backend_image_data(app: AppHandle, filename: String) -> Result<String, String> {
    commands::backend_image_data(&app_paths(&app)?, &filename)
}

#[tauri::command]
async fn backend_probe_video(app: AppHandle, file_path: String) -> Result<String, String> {
    let response = commands::backend_probe_video(&app_paths(&app)?, &file_path)?;
    serde_json::to_string(&response).map_err(|error| error.to_string())
}

#[tauri::command]
fn default_template_save_path(app: tauri::AppHandle, filename: String) -> Result<String, String> {
    let mut path = app.path().document_dir().map_err(|e| e.to_string())?;
    path.push("OVRLEY");
    std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
    path.push(filename);
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
fn write_template_file(path: String, contents: String) -> Result<String, String> {
    let path_buf = PathBuf::from(&path);

    if let Some(parent) = path_buf.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    std::fs::write(&path_buf, contents).map_err(|e| e.to_string())?;
    Ok(path)
}

#[tauri::command]
fn write_parse_debug_file(filename: String, contents: String) -> Result<String, String> {
    let mut path = source_repo_root();
    path.push("app");
    path.push("debug");
    std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
    path.push(filename);

    std::fs::write(&path, contents).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().to_string())
}

pub fn run() {
    tauri::Builder::default()
        .manage(BackendState {
            render_controller: RenderController::default(),
        })
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            backend_health,
            backend_current_os,
            backend_list_system_fonts,
            backend_demo,
            backend_render,
            backend_progress,
            backend_cancel,
            backend_list_templates,
            backend_get_template,
            backend_open_downloads,
            backend_open_video,
            backend_image_data,
            backend_probe_video,
            default_template_save_path,
            write_template_file,
            write_parse_debug_file
        ])
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
