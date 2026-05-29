/**
 * Creates the editor slice used by the application store.
 *
 * This slice owns preview/editor state plus the canonical overlay selection
 * model shared between the canvas and widget sidebar.
 */

import {
  applyConfigOriginatedSceneTiming,
  applyTimelineOriginatedSceneTiming,
  cloneSerializable,
  DEFAULT_CONFIG,
  hasSerializableChanged,
} from '../store-utils'
import { incrementPreviewPerfCounter, previewPerfCounterName } from '../../lib/previewPerf'
import { buildConfigWidgets } from '../../lib/widget-config'

/**
 * Filters and orders selection IDs to match the current widget order.
 *
 * @param {string[]} widgetIds - Requested selected widget IDs.
 * @param {string[]} orderedWidgetIds - Canonical widget order for the config.
 * @returns {string[]} Normalized selection IDs.
 */
function normalizeSelectionIds(widgetIds, orderedWidgetIds) {
  const idSet = new Set((widgetIds || []).filter(Boolean))
  return orderedWidgetIds.filter((widgetId) => idSet.has(widgetId))
}

/**
 * Picks the primary widget from the normalized selection list.
 *
 * @param {string[]} widgetIds - Normalized selected widget IDs.
 * @param {string|null} [preferredId=null] - Preferred primary widget ID.
 * @returns {string|null} Primary widget ID.
 */
function getPrimarySelectionId(widgetIds, preferredId = null) {
  if (preferredId && widgetIds.includes(preferredId)) {
    return preferredId
  }

  return widgetIds[widgetIds.length - 1] ?? null
}

/**
 * Builds widget wrappers for selection normalization and remapping.
 *
 * @param {object|null} config - Overlay config to inspect.
 * @returns {Array<{ id: string, data: object }>} Config widgets.
 */
function getConfigWidgets(config) {
  return buildConfigWidgets(config)
}

/**
 * Rebuilds selection after a config replacement.
 *
 * When config helpers preserve widget object references, this remaps selection
 * to the widget's new index-derived ID so deleting or reordering sibling
 * widgets does not silently jump selection to a different widget.
 *
 * @param {object} options
 * @param {object|null} options.previousConfig - Previous config value.
 * @param {object|null} options.nextConfig - Incoming config value.
 * @param {string[]} options.selectedWidgetIds - Previously selected widget IDs.
 * @param {string|null} options.selectedWidgetId - Previously selected primary widget ID.
 * @returns {{ selectedWidgetIds: string[], selectedWidgetId: string|null }} Next selection state.
 */
function deriveSelectionStateFromConfigChange({ previousConfig, nextConfig, selectedWidgetIds, selectedWidgetId }) {
  const previousWidgetsById = new Map(getConfigWidgets(previousConfig).map((widget) => [widget.id, widget.data]))
  const nextWidgets = getConfigWidgets(nextConfig)
  const orderedWidgetIds = nextWidgets.map((widget) => widget.id)

  if (!orderedWidgetIds.length) {
    return {
      selectedWidgetIds: [],
      selectedWidgetId: null,
    }
  }

  const remapWidgetId = (widgetId) => {
    const previousData = previousWidgetsById.get(widgetId)
    if (!previousData) {
      return null
    }

    return nextWidgets.find((widget) => widget.data === previousData)?.id ?? null
  }

  const remappedWidgetIds = normalizeSelectionIds((selectedWidgetIds || []).map(remapWidgetId).filter(Boolean), orderedWidgetIds)
  const remappedPrimaryId = remapWidgetId(selectedWidgetId)

  if (remappedWidgetIds.length) {
    return {
      selectedWidgetIds: remappedWidgetIds,
      selectedWidgetId: getPrimarySelectionId(remappedWidgetIds, remappedPrimaryId),
    }
  }

  if (remappedPrimaryId) {
    return {
      selectedWidgetIds: [remappedPrimaryId],
      selectedWidgetId: remappedPrimaryId,
    }
  }

  const fallbackWidgetId = selectedWidgetId ? orderedWidgetIds[0] : orderedWidgetIds[orderedWidgetIds.length - 1]

  return {
    selectedWidgetIds: [fallbackWidgetId],
    selectedWidgetId: fallbackWidgetId,
  }
}

/**
 * Applies an explicit selection intent against the current config.
 *
 * @param {object} state - Current Zustand state.
 * @param {string[]} widgetIds - Requested selection IDs.
 * @param {string|null} [preferredWidgetId=null] - Preferred primary widget ID.
 */
function applyWidgetSelectionIntent(state, widgetIds, preferredWidgetId = null) {
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
      const isDifferent = currentState.lastRenderedConfig ? hasSerializableChanged(val, currentState.lastRenderedConfig) : false
      const nextSelectionState = deriveSelectionStateFromConfigChange({
        previousConfig: currentState.config,
        nextConfig: val,
        selectedWidgetIds: currentState.selectedWidgetIds,
        selectedWidgetId: currentState.selectedWidgetId,
      })

      set((state) => {
        state.config = val
        state.selectedWidgetIds = nextSelectionState.selectedWidgetIds
        state.selectedWidgetId = nextSelectionState.selectedWidgetId
        applyConfigOriginatedSceneTiming(state, val, { previousConfig: currentState.config })
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
        applyTimelineOriginatedSceneTiming(draft, { startSecond: second })
      })
    },

    setEndSecond: (second) => {
      const state = get()
      if (state.endSecond === second) return

      set((draft) => {
        draft.endSecond = second
        applyTimelineOriginatedSceneTiming(draft, { endSecond: second })
      })
    },

    setSelectedSecond: (second) => {
      const safeSecond = normalizePreviewSecond(second)

      set((state) => {
        state.selectedSecond = safeSecond
      })
    },

    setSelectedSecondTransient: (second) => {
      const safeSecond = normalizePreviewSecond(second)

      incrementPreviewPerfCounter(previewPerfCounterName('transient selectedSecond updates'))

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
        applyWidgetSelectionIntent(state, widgetIds, preferredWidgetId)
      }),

    setSelectedWidgetId: (widgetId) =>
      set((state) => {
        applyWidgetSelectionIntent(state, widgetId ? [widgetId] : [], widgetId)
      }),
  }
}
