//! Port of `features/player`: the overlay player's playback, timeline viewport, lanes, gestures and
//! edits at the playhead (plan D6 to D11, D16).

pub mod playback;
pub mod timeline;

use crate::store::Store;

/// The player's hook state (D3, D9): what React keeps in the player hooks' refs and state.
#[derive(Debug, Default)]
pub struct PlayerState {
    /// The cached timeline (D5).
    pub timeline: timeline::TimelineCache,
}

/// The `player` reconcile step: the timeline derivations, then the playback effects (D6) and the
/// viewport rules (D7), in React's effect order.
pub fn reconcile_player(store: &mut Store) {
    timeline::reconcile_timeline(store);
}
