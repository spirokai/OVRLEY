//! Port of `store/slices/createLayoutSlice.js`: the shared left drawer.

use serde::{Deserialize, Serialize};

/// The drawer's tools, serialized as the React constants (`ACTIVITY_TOOL`, …).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LeftDrawerTool {
    Projects,
    Activity,
    Video,
    VideoSync,
    #[default]
    Widgets,
}

impl LeftDrawerTool {
    /// The rail's order.
    pub const ALL: [LeftDrawerTool; 5] = [
        Self::Projects,
        Self::Activity,
        Self::Video,
        Self::VideoSync,
        Self::Widgets,
    ];
}

/// Sync Doctor's tabs. The React slice rejected any other tab at runtime; the enum makes one
/// unrepresentable.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VideoSyncDrawerTab {
    #[default]
    Autodetect,
    Manual,
}

/// The durable drawer preference (`leftDrawer`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DrawerPreference {
    pub pinned: bool,
    pub active_tool: LeftDrawerTool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LayoutSlice {
    pub left_drawer_initialized: bool,
    pub left_drawer_visible: bool,
    pub left_drawer_pinned: bool,
    pub active_left_drawer_tool: LeftDrawerTool,
    pub video_sync_drawer_tab: VideoSyncDrawerTab,
}

impl LayoutSlice {
    /// Establishes canonical drawer state from a normalized preference.
    pub fn initialize_left_drawer(&mut self, preference: DrawerPreference) {
        self.left_drawer_initialized = true;
        self.left_drawer_pinned = preference.pinned;
        self.left_drawer_visible = preference.pinned;
        self.active_left_drawer_tool = preference.active_tool;
    }

    /// Selects a tool and applies the shared drawer visibility rules: selecting the tool already
    /// shown closes an unpinned drawer.
    pub fn select_left_drawer_tool(&mut self, tool: LeftDrawerTool) {
        if self.left_drawer_pinned {
            self.active_left_drawer_tool = tool;
            self.left_drawer_visible = true;
            return;
        }
        if self.left_drawer_visible && self.active_left_drawer_tool == tool {
            self.left_drawer_visible = false;
            return;
        }
        self.active_left_drawer_tool = tool;
        self.left_drawer_visible = true;
    }

    /// Opens Sync Doctor directly on its manual-sync tab.
    pub fn open_manual_video_sync(&mut self) {
        self.active_left_drawer_tool = LeftDrawerTool::VideoSync;
        self.left_drawer_visible = true;
        self.video_sync_drawer_tab = VideoSyncDrawerTab::Manual;
    }

    pub fn set_video_sync_drawer_tab(&mut self, tab: VideoSyncDrawerTab) {
        self.video_sync_drawer_tab = tab;
    }

    /// Dismisses the drawer only while it is a temporary workspace overlay.
    pub fn dismiss_left_drawer_overlay(&mut self) {
        if !self.left_drawer_pinned {
            self.left_drawer_visible = false;
        }
    }

    /// Changes drawer mode while preserving a visible active drawer.
    pub fn set_left_drawer_pinned(&mut self, pinned: bool) {
        self.left_drawer_pinned = pinned;
        self.left_drawer_visible = true;
    }

    /// The drawer is shown as a temporary overlay over the workspace.
    pub fn is_unpinned_overlay(&self) -> bool {
        self.left_drawer_visible && !self.left_drawer_pinned
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn initialized(pinned: bool) -> LayoutSlice {
        let mut state = LayoutSlice::default();
        state.initialize_left_drawer(DrawerPreference {
            pinned,
            active_tool: LeftDrawerTool::Widgets,
        });
        state
    }

    #[test]
    fn initialization_restores_a_pinned_drawer_visibly_with_its_active_tool() {
        let state = initialized(true);
        assert_eq!(state.active_left_drawer_tool, LeftDrawerTool::Widgets);
        assert!(state.left_drawer_initialized);
        assert!(state.left_drawer_pinned);
        assert!(state.left_drawer_visible);
    }

    #[test]
    fn selecting_the_active_tool_toggles_only_an_unpinned_drawer() {
        let mut state = initialized(false);

        state.select_left_drawer_tool(LeftDrawerTool::Widgets);
        assert!(state.left_drawer_visible);

        state.select_left_drawer_tool(LeftDrawerTool::Widgets);
        assert!(!state.left_drawer_visible);

        state.set_left_drawer_pinned(true);
        state.select_left_drawer_tool(LeftDrawerTool::Widgets);
        assert!(state.left_drawer_visible);
    }

    #[test]
    fn dismissal_and_unpinning_preserve_the_canonical_visibility_rules() {
        let mut state = initialized(true);

        state.dismiss_left_drawer_overlay();
        assert!(state.left_drawer_visible);

        state.set_left_drawer_pinned(false);
        assert!(!state.left_drawer_pinned);
        assert!(state.left_drawer_visible);

        state.dismiss_left_drawer_overlay();
        assert!(!state.left_drawer_visible);
    }

    #[test]
    fn opens_sync_doctor_directly_on_manual_sync() {
        let mut state = LayoutSlice::default();

        state.open_manual_video_sync();

        assert_eq!(state.active_left_drawer_tool, LeftDrawerTool::VideoSync);
        assert!(state.left_drawer_visible);
        assert_eq!(state.video_sync_drawer_tab, VideoSyncDrawerTab::Manual);
    }

    #[test]
    fn accepts_only_canonical_sync_doctor_tabs() {
        let mut state = LayoutSlice::default();
        state.set_video_sync_drawer_tab(serde_json::from_str("\"manual\"").unwrap());
        assert_eq!(state.video_sync_drawer_tab, VideoSyncDrawerTab::Manual);
        assert!(serde_json::from_str::<VideoSyncDrawerTab>("\"settings\"").is_err());
    }

    #[test]
    fn tools_serialize_as_the_react_constants() {
        let names: Vec<String> = LeftDrawerTool::ALL
            .iter()
            .map(|tool| serde_json::to_string(tool).unwrap())
            .collect();
        assert_eq!(
            names,
            [
                "\"projects\"",
                "\"activity\"",
                "\"video\"",
                "\"videoSync\"",
                "\"widgets\""
            ]
        );
    }
}
