/**
 * Creates the create media slice Zustand slice used by the application store.
 */

import { DEFAULT_RENDER_PROGRESS } from '../store-utils'
import { createActivitySummary } from '@/lib/activity/activity-summary'

/**
 * Media slice — activity summary, render progress, error state, video/gpx filenames.
 * @param {Function} set - Zustand setter.
 * @param {Function} get - Zustand getter.
 */
export function createMediaSlice(set, get) {
  return {
    isProcessing: false,
    importingVideo: false,
    renderingVideo: false,
    renderSubmissionTarget: null,
    renderCancellationTarget: null,
    errorMessage: null,
    activitySource: null,
    activitySummary: null,
    parsedActivity: null, // external activity file; embedded video telemetry stays on each imported video
    activeRenderId: null,
    activeRenderOutputPath: null,
    renderProgress: { ...DEFAULT_RENDER_PROGRESS },

    setProcessing: (processing) =>
      set((state) => {
        state.isProcessing = processing
      }),

    setImportingVideo: (importing) =>
      set((state) => {
        state.importingVideo = importing
      }),

    setRenderSubmissionTarget: (target) => set({ renderSubmissionTarget: target }),
    setRenderCancellationTarget: (target) => set({ renderCancellationTarget: target }),

    startRenderSession: (renderId, outputPath, progress = {}) => {
      const nextProgress = {
        ...DEFAULT_RENDER_PROGRESS,
        ...progress,
        renderId,
        status: 'rendering',
      }
      nextProgress.percent = nextProgress.total > 0 ? Math.round((nextProgress.current / nextProgress.total) * 100) : 0

      set((state) => {
        state.activeRenderId = renderId
        state.activeRenderOutputPath = outputPath
        state.renderingVideo = true
        state.renderProgress = nextProgress
      })
    },

    clearRenderSession: () =>
      set((state) => {
        state.activeRenderId = null
        state.activeRenderOutputPath = null
        state.renderingVideo = false
        state.renderCancellationTarget = null
      }),

    setRenderProgress: (progress) => {
      const percent = progress.total > 0 ? Math.round((progress.current / progress.total) * 100) : 0

      set((state) => {
        state.renderProgress = {
          ...DEFAULT_RENDER_PROGRESS,
          ...progress,
          percent,
        }
      })
    },

    setErrorMessage: (message) =>
      set((state) => {
        state.errorMessage = message
      }),

    clearError: () =>
      set((state) => {
        state.errorMessage = null
      }),

    setActivitySource: (source) => {
      set((state) => {
        state.activitySource = source
      })
    },

    setActivitySummary: (activity, { computeVideoSync = true } = {}) => {
      set((state) => {
        state.activitySummary = activity ? createActivitySummary(activity) : null
      })

      if (computeVideoSync) {
        get().computeVideoSync()
      }
    },

    clearActivitySummary: () => {
      get().clearActivityFile({ clearFilename: false })
    },

    /** Stores the full parsed activity object (with all metric series) for preview and rendering. */
    setParsedActivity: (activity) =>
      set((state) => {
        state.parsedActivity = activity
      }),

    activateActivityFile: (activity) => {
      set((state) => {
        state.parsedActivity = activity
      })
      get().clearVideoSyncForActivity()
      get().setActivitySummary(activity)
    },

    /** Removes the external activity; clips fall back to their own embedded telemetry and detected-time placement. */
    clearActivityFile: ({ clearFilename = true } = {}) => {
      if (get().parsedActivity === null) return

      set((state) => {
        if (clearFilename) {
          state.activitySource = null
        }
        state.parsedActivity = null
        state.activitySummary = null
      })
      get().clearVideoSyncForActivity()
      get().computeVideoSync()
    },

    setDemoActivity: () =>
      set((state) => {
        const demoDuration = 7946
        state.activitySource = null
        state.fallbackDurationSeconds = demoDuration
        state.startSecond = 0
        state.endSecond = demoDuration
        state.selectedSecond = 0
      }),
  }
}
