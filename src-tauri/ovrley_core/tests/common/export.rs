//! Full scene frames drawn through the export host of the shared widget engine.
#![allow(dead_code)]

use super::{builders::minimal_dense_activity, test_config::repo_git_root};
use ovrley_core::{
    activity::schema::ParsedActivity, debug::RenderProfiler, normalize::ValidatedRenderConfig,
    render::render_full_frames,
};

/// The straight-alpha RGBA scene frame of a config without activity samples, at its single
/// covered frame: static widgets drawn exactly as export draws them.
pub fn scene_frame(config: &ValidatedRenderConfig) -> Vec<u8> {
    let activity: ParsedActivity = serde_json::from_value(serde_json::json!({})).unwrap();
    let font_dirs = vec![repo_git_root().join("fonts")];
    render_full_frames(
        &font_dirs,
        config,
        &activity,
        &minimal_dense_activity(),
        &[0],
        &mut RenderProfiler::default(),
    )
    .unwrap()
    .remove(0)
}
