//! One module per feature: its projection, from the store into the feature's Slint global, and the
//! callbacks bound to dispatches (D4, D5).
pub mod app_shell;
pub mod maps;
pub mod overlay_editor;
pub mod player;
pub mod projects;
pub mod render_video;
pub mod scene_settings;
pub mod template_manager;
pub mod toolbar;
pub mod transitions;
pub mod undo_redo;
#[cfg(all(feature = "video-playback", any(target_os = "macos", windows, target_os = "linux")))]
pub mod video_playback;
pub mod video_preview;
pub mod video_sync;
pub mod widget_drawer;
pub mod widget_editor;
