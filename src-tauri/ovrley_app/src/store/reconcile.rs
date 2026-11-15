//! The reconcile steps, in their fixed order.
//!
//! They replace the React effects that react to store changes. Each runs after every action,
//! before the projections. Later phases append their steps here, in the order the React effects
//! depend on each other; the order lives in this one list, never spread across features.

use super::services::Services;
use super::Store;
use crate::features::toolbar;

pub(super) fn run(store: &mut Store, services: &mut Services) {
    // 1. `useDrawerPreference`'s second effect: persist `{pinned, activeTool}` when it changed.
    toolbar::persist_drawer_preference(store, services);
}
