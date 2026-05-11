/**
 * Creates the create editor slice Zustand slice used by the application store.
 */

import {
  beginConfigUpdate,
  cloneSerializable,
  endConfigUpdateSoon,
  hasSerializableChanged,
  isConfigUpdateInProgress,
  readStoredConfig,
  readStoredInt,
  updateConfigPersistence,
} from '../store-utils'
import {
  incrementPreviewPerfCounter,
  previewPerfCounterName,
} from '../../lib/previewPerf'

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
    previewInterpolationEnabled:
      localStorage.getItem('previewInterpolationEnabled') !== 'false',
    hasUnrenderedChanges: false,
    lastRenderedConfig: null,
    dummyDurationSeconds: readStoredInt('dummyDurationSeconds', 73),
    startSecond: readStoredInt('startSecond', 0),
    endSecond: readStoredInt('endSecond', 73),
    selectedSecond: readStoredInt('selectedSecond', 0),
    previewPlaybackState: 'paused',
    previewPlaybackSource: 'timeline',
    previewPlaybackStartedAtSecond: 0,
    config: readStoredConfig(),
    autoRender: localStorage.getItem('autoRender') === 'true',

    setConfig: (val) => {
      const currentState = get()
      const isDifferent = currentState.lastRenderedConfig
        ? hasSerializableChanged(val, currentState.lastRenderedConfig)
        : false

      localStorage.setItem('editorConfig', JSON.stringify(val))

      const wasUpdating = beginConfigUpdate()

      set((state) => {
        state.config = val

        if (val.scene) {
          const hasExistingTimeline =
            state.startSecond !== 0 ||
            state.endSecond !== state.dummyDurationSeconds

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
            if (
              val.scene.start !== undefined &&
              val.scene.start !== state.startSecond
            ) {
              state.startSecond = val.scene.start
              localStorage.setItem('startSecond', val.scene.start.toString())
            }
            if (
              val.scene.end !== undefined &&
              val.scene.end !== state.endSecond
            ) {
              state.endSecond = val.scene.end
              localStorage.setItem('endSecond', val.scene.end.toString())
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

    setAutoRender: (val) => {
      localStorage.setItem('autoRender', val.toString())
      set((state) => {
        state.autoRender = val
      })
    },

    setDummyDurationSeconds: (duration) => {
      localStorage.setItem('dummyDurationSeconds', duration.toString())
      set((state) => {
        state.dummyDurationSeconds = duration
      })
    },

    setStartSecond: (second) => {
      localStorage.setItem('startSecond', second.toString())

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
      localStorage.setItem('endSecond', second.toString())

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

      localStorage.setItem('selectedSecond', safeSecond.toString())
      set((state) => {
        state.selectedSecond = safeSecond
      })
    },

    setSelectedSecondTransient: (second) => {
      const safeSecond = normalizePreviewSecond(second)

      incrementPreviewPerfCounter(
        previewPerfCounterName('transient selectedSecond updates'),
      )

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
        state.previewPlaybackStartedAtSecond = safeSecond
        state.selectedSecond = safeSecond
      })
    },

    pausePreviewPlayback: (second) => {
      const safeSecond = normalizePreviewSecond(second)

      localStorage.setItem('selectedSecond', safeSecond.toString())
      set((state) => {
        state.previewPlaybackState = 'paused'
        state.previewPlaybackStartedAtSecond = safeSecond
        state.selectedSecond = safeSecond
      })
    },

    beginPreviewScrub: (second) => {
      const safeSecond = normalizePreviewSecond(second)

      set((state) => {
        state.previewPlaybackState = 'scrubbing'
        state.previewPlaybackStartedAtSecond = safeSecond
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

      localStorage.setItem('selectedSecond', safeSecond.toString())
      set((state) => {
        state.previewPlaybackState = 'paused'
        state.previewPlaybackStartedAtSecond = safeSecond
        state.selectedSecond = safeSecond
      })
    },

    setPreviewInterpolationEnabled: (enabled) => {
      localStorage.setItem('previewInterpolationEnabled', enabled.toString())
      set((state) => {
        state.previewInterpolationEnabled = enabled
      })
    },

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
