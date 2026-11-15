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
    overlay_editor::drafts::reconcile_widget_drafts(store);
    // The plan's language step is not needed: `change_language_preference` validates, applies and
    // persists `lng` itself, as React's does, and every path (Settings, MCP) goes through it.
}
