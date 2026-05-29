/**
 * Creates the create editor slice Zustand slice used by the application store.
 */

import {
  beginConfigUpdate,
  cloneSerializable,
  DEFAULT_CONFIG,
  endConfigUpdateSoon,
  hasSerializableChanged,
  isConfigUpdateInProgress,
  updateConfigPersistence,
} from '../store-utils'
import { incrementPreviewPerfCounter, previewPerfCounterName } from '../../lib/previewPerf'

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

      const wasUpdating = beginConfigUpdate()

      set((state) => {
        state.config = val

        if (val.scene) {
          const hasExistingTimeline = state.startSecond !== 0 || state.endSecond !== state.dummyDurationSeconds

          if (!hasExistingTimeline) {
            if (val.scene.start !== undefined) {
              state.startSecond = val.scene.start
            }
            if (val.scene.end !== undefined) {
              state.endSecond = val.scene.end
            }
            if (val.scene.start !== undefined) {
              state.selectedSecond = val.scene.start
            }
          } else {
            if (val.scene.start !== undefined && val.scene.start !== state.startSecond) {
              state.startSecond = val.scene.start
            }
            if (val.scene.end !== undefined && val.scene.end !== state.endSecond) {
              state.endSecond = val.scene.end
            }
          }
        }

        if (!wasUpdating) {
          state.hasUnrenderedChanges = isDifferent
        }
      })

      endConfigUpdateSoon()
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

        if (!isConfigUpdateInProgress() && draft.config && draft.config.scene) {
          if (draft.config.scene.start === second) return

          draft.config.scene.start = second
          updateConfigPersistence(draft)
        }
      })
    },

    setEndSecond: (second) => {
      const state = get()
      if (state.endSecond === second) return

      set((draft) => {
        draft.endSecond = second

        if (!isConfigUpdateInProgress() && draft.config && draft.config.scene) {
          if (draft.config.scene.end === second) return

          draft.config.scene.end = second
          updateConfigPersistence(draft)
        }
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

    setSelectedWidgetId: (widgetId) =>
      set((state) => {
        state.selectedWidgetId = widgetId
      }),
  }
}
