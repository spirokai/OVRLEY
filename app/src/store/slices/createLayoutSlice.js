export const ACTIVITY_TOOL = 'activity'
export const VIDEO_TOOL = 'video'
export const WIDGETS_TOOL = 'widgets'

/**
 * Creates the layout slice Zustand slice used by the application store.
 */

/**
 * Creates layout slice.
 *
 * @param {*} set - Zustand setter callback.
 * @param {*} _get - Zustand getter callback (unused).
 * @returns {object} Derived data structure for downstream use.
 */
export function createLayoutSlice(set, _get) {
  return {
    leftDrawerInitialized: false,
    leftDrawerVisible: false,
    leftDrawerPinned: false,
    activeLeftDrawerTool: WIDGETS_TOOL,

    /**
     * Establishes canonical drawer state from a normalized preference.
     *
     * @param {{pinned: boolean, activeTool: string}} preference - Normalized durable preference.
     */
    initializeLeftDrawer: (preference) =>
      set((state) => {
        state.leftDrawerInitialized = true
        state.leftDrawerPinned = preference.pinned
        state.leftDrawerVisible = preference.pinned
        state.activeLeftDrawerTool = preference.activeTool
      }),

    /**
     * Selects a tool and applies the shared drawer visibility rules.
     *
     * @param {string} tool - Canonical tool identifier.
     */
    selectLeftDrawerTool: (tool) =>
      set((state) => {
        if (state.leftDrawerPinned) {
          state.activeLeftDrawerTool = tool
          state.leftDrawerVisible = true
          return
        }

        if (state.leftDrawerVisible && state.activeLeftDrawerTool === tool) {
          state.leftDrawerVisible = false
          return
        }

        state.activeLeftDrawerTool = tool
        state.leftDrawerVisible = true
      }),

    /**
     * Dismisses the drawer only while it is a temporary workspace overlay.
     */
    dismissLeftDrawerOverlay: () =>
      set((state) => {
        if (!state.leftDrawerPinned) state.leftDrawerVisible = false
      }),

    /**
     * Changes drawer mode while preserving a visible active drawer.
     *
     * @param {boolean} pinned - Whether the drawer participates in shell layout.
     */
    setLeftDrawerPinned: (pinned) =>
      set((state) => {
        state.leftDrawerPinned = pinned
        state.leftDrawerVisible = true
      }),
  }
}
