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
    widgetDrawerOpen: false,

    /**
     * Toggles the widget drawer open or closed.
     */
    toggleWidgetDrawer: () =>
      set((state) => {
        state.widgetDrawerOpen = !state.widgetDrawerOpen
      }),
  }
}
