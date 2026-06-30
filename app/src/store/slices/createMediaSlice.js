/**
 * Creates the create media slice Zustand slice used by the application store.
 */

import { DEFAULT_RENDER_PROGRESS, syncSceneTimingToConfig } from '../store-utils'
import { getCourseWidgetDimensions } from '@/features/widget-editor/utils/widgetUtils'

function getDurationSeconds(activity) {
  const duration = Number(activity?.trim_end_seconds ?? activity?.metadata?.duration_seconds ?? 0)
  return Number.isFinite(duration) ? duration : 0
}

function applyParsedDataToScene(state, activity) {
  const durationSeconds = getDurationSeconds(activity)
  if (durationSeconds <= 0) {
    return
  }

  const wholeSeconds = Math.floor(durationSeconds)
  state.fallbackDurationSeconds = wholeSeconds
  state.startSecond = 0
  state.endSecond = wholeSeconds
  state.selectedSecond = 0
  syncSceneTimingToConfig(state, { startSecond: 0, endSecond: wholeSeconds })

  if (state.config?.plots) {
    const coursePoints = activity?.sample_course_points
    for (const plot of state.config.plots) {
      if (plot.value === 'course' && coursePoints) {
        const dims = getCourseWidgetDimensions(coursePoints)
        if (dims) {
          plot.width = dims.width
          plot.height = dims.height
        }
      }
    }
  }
}

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
    parsedActivitySource: null, // 'activity-file' | 'video-telemetry' | null
    stashedVideoTelemetry: null,
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

    setActivitySummary: (activity, { computeVideoSync = true } = {}) => {
      let summary = null
      if (activity) {
        summary = {
          durationSeconds: activity.metadata?.duration_seconds ?? 0,
          endTime: activity.metadata?.end_time ?? null,
          extendedAttributes: activity.extended_attributes || [],
          fileFormat: activity.file_format || null,
          fileName: activity.file_name || null,
          sampleCount: activity.metadata?.sample_count ?? 0,
          syncTime: activity.sync_time ?? null,
          totalDistanceMeters: activity.metadata?.total_distance_m ?? 0,
          validAttributes: activity.valid_attributes || [],
        }
      }

      set((state) => {
        state.activitySummary = summary
      })

      if (computeVideoSync && get().computeVideoSync) {
        get().computeVideoSync(summary)
      }
    },

    clearActivitySummary: () => {
      get().clearActivityFile({
        restoreVideoTelemetry: false,
        clearFilename: false,
      })
    },

    /** Stores the full parsed activity object (with all metric series) for preview and rendering. */
    setParsedActivity: (activity) =>
      set((state) => {
        state.parsedActivity = activity
      }),

    activateActivityFile: (activity) => {
      set((state) => {
        if (state.parsedActivitySource === 'video-telemetry') {
          state.stashedVideoTelemetry = state.parsedActivity
        }
        state.parsedActivity = activity
        state.parsedActivitySource = 'activity-file'
      })
      get().setActivitySummary(activity)
    },

    loadVideoTelemetry: (activity) => {
      const source = get().parsedActivitySource
      if (source === 'activity-file') {
        set((state) => {
          state.stashedVideoTelemetry = activity
        })
      } else {
        set((state) => {
          state.parsedActivity = activity
          state.parsedActivitySource = 'video-telemetry'
          state.stashedVideoTelemetry = null
          state.activityFilename = null
          state.videoSyncOffsetSeconds = 0
          state.videoSyncWarning = null
          applyParsedDataToScene(state, activity)
        })
        get().setActivitySummary(activity, { computeVideoSync: false })
      }
    },

    clearActivityFile: ({ restoreVideoTelemetry = true, clearFilename = true } = {}) => {
      const { parsedActivitySource, stashedVideoTelemetry } = get()

      if (parsedActivitySource !== 'activity-file') return

      set((state) => {
        if (clearFilename) {
          state.activityFilename = null
        }
      })

      if (restoreVideoTelemetry && stashedVideoTelemetry) {
        set((state) => {
          state.parsedActivity = stashedVideoTelemetry
          state.parsedActivitySource = 'video-telemetry'
          state.stashedVideoTelemetry = null
          state.videoSyncOffsetSeconds = 0
          state.videoSyncWarning = null
          applyParsedDataToScene(state, stashedVideoTelemetry)
        })
        get().setActivitySummary(get().parsedActivity, { computeVideoSync: false })
      } else {
        set((state) => {
          state.parsedActivity = null
          state.parsedActivitySource = null
          state.activitySummary = null
        })
      }
    },

    clearVideoTelemetry: () => {
      const { parsedActivitySource } = get()

      set((state) => {
        state.stashedVideoTelemetry = null
      })

      if (parsedActivitySource === 'video-telemetry') {
        set((state) => {
          state.parsedActivity = null
          state.parsedActivitySource = null
          state.activitySummary = null
        })
      }
    },

    syncVideoMetadata: () => {
      const { parsedActivitySource, activitySummary, computeVideoSync } = get()

      if (parsedActivitySource === 'activity-file') {
        if (computeVideoSync) {
          computeVideoSync(activitySummary)
        }
      } else {
        set((state) => {
          state.videoSyncOffsetSeconds = 0
          state.videoSyncWarning = null
        })
      }
    },

    setDemoActivity: () =>
      set((state) => {
        const demoDuration = 7946
        state.activityFilename = 'demo.gpxinit'
        state.fallbackDurationSeconds = demoDuration
        state.startSecond = 0
        state.endSecond = demoDuration
        state.selectedSecond = 0
      }),
  }
}
