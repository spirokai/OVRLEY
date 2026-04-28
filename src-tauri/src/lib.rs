#[cfg_attr(mobile, tauri::mobile_entry_point)]
use cyclemetry_core::commands::{self, AppPaths};
use cyclemetry_core::encode::video::RenderController;
use std::path::PathBuf;
use tauri::Manager;

struct BackendState {
    render_controller: RenderController,
}

fn app_paths() -> Result<AppPaths, String> {
    let mut root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    root.pop();
    let paths = AppPaths::from_repo_root(root);
    paths.ensure_dirs()?;
    Ok(paths)
}

#[tauri::command]
async fn backend_health() -> Result<String, String> {
    let response = commands::backend_health(&app_paths()?);
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
    config_json: String,
    parsed_activity_json: String,
    second: u32,
) -> Result<String, String> {
    let response =
        commands::backend_demo(&app_paths()?, &config_json, &parsed_activity_json, second)?;
    serde_json::to_string(&response).map_err(|error| error.to_string())
}

#[tauri::command]
async fn backend_render(
    state: tauri::State<'_, BackendState>,
    config_json: String,
    parsed_activity_json: String,
) -> Result<String, String> {
    let response = commands::backend_render(
        &app_paths()?,
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
async fn backend_open_downloads() -> Result<String, String> {
    let response = commands::backend_open_downloads(&app_paths()?)?;
    serde_json::to_string(&response).map_err(|error| error.to_string())
}

#[tauri::command]
async fn backend_open_video(filename: String) -> Result<String, String> {
    let response = commands::backend_open_video(&app_paths()?, &filename)?;
    serde_json::to_string(&response).map_err(|error| error.to_string())
}

#[tauri::command]
async fn backend_load_gpx(path: String) -> Result<String, String> {
    let response = commands::backend_load_gpx(&app_paths()?, &path)?;
    serde_json::to_string(&response).map_err(|error| error.to_string())
}

#[tauri::command]
async fn backend_upload(file_data: Vec<u8>, filename: String) -> Result<String, String> {
    let response = commands::backend_upload(&app_paths()?, &file_data, &filename)?;
    serde_json::to_string(&response).map_err(|error| error.to_string())
}

#[tauri::command]
async fn backend_list_templates() -> Result<String, String> {
    let response = commands::backend_list_templates(&app_paths()?)?;
    serde_json::to_string(&response).map_err(|error| error.to_string())
}

#[tauri::command]
async fn backend_save_template(config: String, filename: String) -> Result<String, String> {
    let response = commands::backend_save_template(&app_paths()?, &filename, &config)?;
    serde_json::to_string(&response).map_err(|error| error.to_string())
}

#[tauri::command]
async fn backend_get_template(filename: String) -> Result<String, String> {
    commands::backend_get_template(&app_paths()?, &filename)
}

#[tauri::command]
async fn backend_open_templates() -> Result<String, String> {
    let response = commands::backend_open_templates(&app_paths()?)?;
    serde_json::to_string(&response).map_err(|error| error.to_string())
}

#[tauri::command]
async fn backend_cancel(state: tauri::State<'_, BackendState>) -> Result<String, String> {
    let response = commands::backend_cancel(&state.render_controller);
    serde_json::to_string(&response).map_err(|error| error.to_string())
}

#[tauri::command]
fn get_image_url(filename: String) -> String {
    filename
}

#[tauri::command]
async fn backend_socket_ready() -> bool {
    true
}

#[tauri::command]
async fn backend_image_data(filename: String) -> Result<String, String> {
    commands::backend_image_data(&app_paths()?, &filename)
}

#[tauri::command]
fn default_template_save_path(app: tauri::AppHandle, filename: String) -> Result<String, String> {
    let mut path = app.path().document_dir().map_err(|e| e.to_string())?;
    path.push("Cyclemetry");
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
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop();
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
            backend_load_gpx,
            backend_list_templates,
            backend_get_template,
            backend_save_template,
            backend_open_templates,
            backend_open_downloads,
            backend_open_video,
            backend_upload,
            backend_socket_ready,
            get_image_url,
            backend_image_data,
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
