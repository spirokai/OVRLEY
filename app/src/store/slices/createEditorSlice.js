/**
 * Creates the editor slice used by the application store.
 *
 * This slice owns preview/editor state plus the canonical overlay selection
 * model shared between the canvas and widget sidebar.
 */

import { cloneSerializable, DEFAULT_CONFIG, hasSerializableChanged, syncSceneTimingFromConfig, syncSceneTimingToConfig } from '../store-utils'
import { ensureWidgetIdsInConfig } from '../../lib/widget/widget-config'
import { buildConfigWidgets } from '../../lib/widget/widget-presentation'
import { getPrimarySelectionId, normalizeSelectionIds } from '../../features/overlay-editor/utils/overlayEditorHelpers'

/**
 * Builds widget wrappers for selection normalization.
 *
 * @param {object|null} config - Overlay config to inspect.
 * @returns {Array<{ id: string, data: object }>} Config widgets.
 */
function getConfigWidgets(config) {
  return buildConfigWidgets(config)
}

/**
 * Reconciles selection after a config replacement.
 *
 * Widget ids are durable, so selection only needs to retain ids that still
 * exist in the incoming config and then choose an appropriate primary widget.
 *
 * @param {object} options
 * @param {object|null} options.nextConfig - Incoming config value.
 * @param {string[]} options.selectedWidgetIds - Previously selected widget IDs.
 * @param {string|null} options.selectedWidgetId - Previously selected primary widget ID.
 * @returns {{ selectedWidgetIds: string[], selectedWidgetId: string|null }} Next selection state.
 */
function reconcileSelection({ nextConfig, selectedWidgetIds, selectedWidgetId }) {
  const nextWidgets = getConfigWidgets(nextConfig)
  const orderedWidgetIds = nextWidgets.map((widget) => widget.id)

  if (!orderedWidgetIds.length) {
    return {
      selectedWidgetIds: [],
      selectedWidgetId: null,
    }
  }

  const retainedWidgetIds = normalizeSelectionIds(selectedWidgetIds || [], orderedWidgetIds)

  if (retainedWidgetIds.length) {
    return {
      selectedWidgetIds: retainedWidgetIds,
      selectedWidgetId: getPrimarySelectionId(retainedWidgetIds, selectedWidgetId),
    }
  }

  if (selectedWidgetId && orderedWidgetIds.includes(selectedWidgetId)) {
    return {
      selectedWidgetIds: [selectedWidgetId],
      selectedWidgetId,
    }
  }

  const fallbackWidgetId = selectedWidgetId ? orderedWidgetIds[0] : orderedWidgetIds[orderedWidgetIds.length - 1]

  return {
    selectedWidgetIds: [fallbackWidgetId],
    selectedWidgetId: fallbackWidgetId,
  }
}

/**
 * Sets widget selection against the current config order.
 *
 * @param {object} state - Current Zustand state.
 * @param {string[]} widgetIds - Requested selection IDs.
 * @param {string|null} [preferredWidgetId=null] - Preferred primary widget ID.
 */
function setWidgetSelectionState(state, widgetIds, preferredWidgetId = null) {
  const orderedWidgetIds = getConfigWidgets(state.config).map((widget) => widget.id)
  const nextSelectedWidgetIds = normalizeSelectionIds(widgetIds, orderedWidgetIds)

  state.selectedWidgetIds = nextSelectedWidgetIds
  state.selectedWidgetId = getPrimarySelectionId(nextSelectedWidgetIds, preferredWidgetId)
}

/**
 * Creates editor slice.
 *
 * @param {*} set - Zustand setter callback.
 * @param {*} get - Value for get.
 * @returns {object} Derived data structure for downstream use.
 */
export function createEditorSlice(set, get) {
  const normalizePreviewSecond = (second) => {
    const nextSecond = Number(second)
    return Number.isFinite(nextSecond) ? nextSecond : 0
  }

  return {
    editor: null,
    selectedWidgetId: null,
    selectedWidgetIds: [],
    previewInterpolationEnabled: true,
    hasUnrenderedChanges: false,
    lastRenderedConfig: null,
    dummyDurationSeconds: 73,
    startSecond: 0,
    endSecond: 73,
    selectedSecond: 0,
    previewPlaybackState: 'paused',
    previewPlaybackSource: 'timeline',
    config: cloneSerializable(DEFAULT_CONFIG),
    autoRender: false,

    setConfig: (val) => {
      const currentState = get()
      const normalizedConfig = ensureWidgetIdsInConfig(val)
      const isDifferent = currentState.lastRenderedConfig ? hasSerializableChanged(normalizedConfig, currentState.lastRenderedConfig) : false
      const nextSelectionState = reconcileSelection({
        nextConfig: normalizedConfig,
        selectedWidgetIds: currentState.selectedWidgetIds,
        selectedWidgetId: currentState.selectedWidgetId,
      })

      set((state) => {
        state.config = normalizedConfig
        state.selectedWidgetIds = nextSelectionState.selectedWidgetIds
        state.selectedWidgetId = nextSelectionState.selectedWidgetId
        syncSceneTimingFromConfig(state, normalizedConfig, { previousConfig: currentState.config })
        state.hasUnrenderedChanges = isDifferent
      })
    },

    setHasUnrenderedChanges: (val) =>
      set((state) => {
        state.hasUnrenderedChanges = val
      }),

    setLastRenderedConfig: (config) =>
      set((state) => {
        state.lastRenderedConfig = cloneSerializable(config)
      }),

    setAutoRender: (val) =>
      set((state) => {
        state.autoRender = val
      }),

    setDummyDurationSeconds: (duration) =>
      set((state) => {
        state.dummyDurationSeconds = duration
      }),

    setStartSecond: (second) => {
      const state = get()
      if (state.startSecond === second) return

      set((draft) => {
        draft.startSecond = second
        syncSceneTimingToConfig(draft, { startSecond: second })
      })
    },

    setEndSecond: (second) => {
      const state = get()
      if (state.endSecond === second) return

      set((draft) => {
        draft.endSecond = second
        syncSceneTimingToConfig(draft, { endSecond: second })
      })
    },

    setSelectedSecond: (second) => {
      const safeSecond = normalizePreviewSecond(second)

      set((state) => {
        state.selectedSecond = safeSecond
      })
    },

    startPreviewPlayback: ({ source, second } = {}) => {
      const safeSecond = normalizePreviewSecond(second)
      const safeSource = source === 'video' ? 'video' : 'timeline'

      set((state) => {
        state.previewPlaybackState = 'playing'
        state.previewPlaybackSource = safeSource
        state.selectedSecond = safeSecond
      })
    },

    pausePreviewPlayback: (second) => {
      const safeSecond = normalizePreviewSecond(second)

      set((state) => {
        state.previewPlaybackState = 'paused'
        state.selectedSecond = safeSecond
      })
    },

    beginPreviewScrub: (second) => {
      const safeSecond = normalizePreviewSecond(second)

      set((state) => {
        state.previewPlaybackState = 'scrubbing'
        state.selectedSecond = safeSecond
      })
    },

    updatePreviewScrub: (second) => {
      const safeSecond = normalizePreviewSecond(second)

      set((state) => {
        state.previewPlaybackState = 'scrubbing'
        state.selectedSecond = safeSecond
      })
    },

    commitPreviewScrub: (second) => {
      const safeSecond = normalizePreviewSecond(second)

      set((state) => {
        state.previewPlaybackState = 'paused'
        state.selectedSecond = safeSecond
      })
    },

    setPreviewInterpolationEnabled: (enabled) =>
      set((state) => {
        state.previewInterpolationEnabled = enabled
      }),

    setEditor: (editor) =>
      set((state) => {
        state.editor = editor
      }),

    setWidgetSelection: (widgetIds, preferredWidgetId = null) =>
      set((state) => {
        setWidgetSelectionState(state, widgetIds, preferredWidgetId)
      }),

    setSelectedWidgetId: (widgetId) =>
      set((state) => {
        setWidgetSelectionState(state, widgetId ? [widgetId] : [], widgetId)
      }),
  }
}
