//! The reconcile steps, in their fixed order.
//!
//! They replace the React effects that react to store changes. Each runs after every action,
//! before the projections. Later phases append their steps here, in the order the React effects
//! depend on each other; the order lives in this one list, never spread across features.

use super::services::Services;
use super::Store;
use crate::features::{overlay_editor, toolbar};

pub(super) fn run(store: &mut Store, services: &mut Services) {
    // 1. `useDrawerPreference`'s second effect: persist `{pinned, activeTool}` when it changed.
    toolbar::persist_drawer_preference(store, services);
    // 2. `useWidgetDraftState`'s reset: a new document clears the drafts, and a new document or
    //    selection ends the gesture's patches (D7).
    overlay_editor::interaction::reconcile_gesture(store);
    overlay_editor::drafts::reconcile_widget_drafts(store);
    //    The map view mode ends when its map is gone or deselected, committing its patch (phase 5 D10).
    crate::features::maps::reconcile_map_view(store);
    // 3. `useHeldValue`: the widgets tab's open item holds while a marquee runs.
    crate::features::widget_editor::reconcile_open_widget(store);
    //    The open label's font capabilities, for its weight and italic controls (phase 5 D6).
    crate::features::widget_editor::reconcile_label_typography(store, services);
    //    A label-font edit of the global defaults applies once its family is known (phase 5 D6).
    crate::features::scene_settings::apply_pending_font_edit(store);
    // 4. The durable template state behind the save status, cached by pointers.
    crate::features::template_manager::reconcile_durable_state(store);
    //    The project status, from the durable state and the project-owned state (Phase 6A D11).
    crate::features::projects::reconcile_project_status(store);
    // 5. The player: the cached timeline, the playback effects and the viewport rules (phase 4).
    crate::features::player::reconcile_player(store);
    //    The filmstrips of the clips and the zoom windows the view wants (phase 4 D12).
    crate::features::video_preview::filmstrips::reconcile_filmstrips(store, services);
    //    The blends of the program's transitions (Phase 6B D8).
    crate::features::video_preview::transition_proxies::reconcile_transition_proxies(store, services);
    //    Sync Doctor: detection and the reruns of a search (Phase 8 D3).
    crate::features::video_sync::reconcile_video_sync(store, services);
    // 6. The preview: marked dirty when one of its inputs changed; `ovrley_ui` runs the frame (D9).
    crate::features::widget_preview::reconcile_widget_preview(store);
    // 7. The render dialog: its codec and update-rate rules, the inspection and the export review
    //    (Phase 7 D4, D5).
    crate::features::render_video::reconcile_render_dialog(store, services);
    // The plan's language step is not needed: `change_language_preference` validates, applies and
    // persists `lng` itself, as React's does, and every path (Settings, MCP) goes through it.
}
