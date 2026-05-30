/**
 * Creates the create media slice Zustand slice used by the application store.
 */

import { DEFAULT_RENDER_PROGRESS } from '../store-utils'

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
    errorMessage: null,
    videoFilename: null,
    activityFilename: null,
    activitySummary: null,
    parsedActivity: null,
    activeRenderId: null,
    renderProgress: { ...DEFAULT_RENDER_PROGRESS },

    setProcessing: (processing) =>
      set((state) => {
        state.isProcessing = processing
      }),

    setImportingVideo: (importing) =>
      set((state) => {
        state.importingVideo = importing
      }),

    setRenderingVideo: (rendering) =>
      set((state) => {
        state.renderingVideo = rendering
      }),

    setActiveRenderId: (renderId) =>
      set((state) => {
        state.activeRenderId = renderId
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

    setVideoFilename: (filename) =>
      set((state) => {
        state.videoFilename = filename
      }),

    setActivityFilename: (filename) => {
      set((state) => {
        state.activityFilename = filename
      })
    },

    setActivityFilenameFromFile: (file) => {
      set((state) => {
        state.activityFilename = file?.name || null
      })
    },

    setActivitySummary: (activity) => {
      let summary = null
      if (activity) {
        summary = {
          durationSeconds: activity.metadata?.duration_seconds ?? 0,
          endTime: activity.metadata?.end_time ?? null,
          extendedAttributes: activity.extended_attributes || [],
          fileFormat: activity.file_format || null,
          fileName: activity.file_name || null,
          sampleCount: activity.metadata?.sample_count ?? 0,
          startTime: activity.metadata?.start_time ?? null,
          totalDistanceMeters: activity.metadata?.total_distance_m ?? 0,
          validAttributes: activity.valid_attributes || [],
        }
      }

      set((state) => {
        state.activitySummary = summary
      })

      if (get().computeVideoSync) {
        get().computeVideoSync(summary)
      }
    },

    clearActivitySummary: () => {
      set((state) => {
        state.activitySummary = null
        state.parsedActivity = null
      })
    },

    /** Stores the full parsed activity object (with all metric series) for preview and rendering. */
    setParsedActivity: (activity) =>
      set((state) => {
        state.parsedActivity = activity
      }),

    setDemoActivity: () =>
      set((state) => {
        const demoDuration = 7946
        state.activityFilename = 'demo.gpxinit'
        state.dummyDurationSeconds = demoDuration
        state.startSecond = 0
        state.endSecond = demoDuration
        state.selectedSecond = 0
      }),
  }
}
