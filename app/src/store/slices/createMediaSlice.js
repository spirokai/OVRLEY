/**
 * Creates the create media slice Zustand slice used by the application store.
 */

import { DEFAULT_RENDER_PROGRESS, syncSceneTimingToConfig } from '../store-utils'
import { getCourseWidgetDimensions } from '@/lib/widget/widget-defaults'
import { WIDGET_TYPE_DEFINITIONS } from '@/lib/widget/widget-catalog'
import { releasePreviewActivity } from '@/api/backend'

function previewActivityIds(state) {
  const ids = new Set()
  for (const activity of [state.parsedActivity, state.stashedVideoTelemetry]) {
    // Browser-only activity data has no native snapshot to release.
    if (activity !== null && activity.preview_activity_id !== undefined) ids.add(activity.preview_activity_id)
  }
  return ids
}

function reportPreviewActivityReleaseError(error) {
  console.error('Unable to release preview activity', error)
}

function setActivityState(set, get, update) {
  const previousIds = previewActivityIds(get())
  set(update)
  const retainedIds = previewActivityIds(get())
  for (const id of previousIds) {
    if (!retainedIds.has(id)) releasePreviewActivity(id).catch(reportPreviewActivityReleaseError)
  }
}

function getDurationSeconds(activity) {
  const duration = Number(activity?.trim_end_seconds ?? activity?.metadata?.duration_seconds ?? 0)
  return Number.isFinite(duration) ? duration : 0
}

function applyParsedDataToScene(state, activity) {
  const durationSeconds = getDurationSeconds(activity)
  if (durationSeconds <= 0) {
    return
  }

  state.fallbackDurationSeconds = durationSeconds
  state.startSecond = 0
  state.endSecond = durationSeconds
  state.selectedSecond = 0
  syncSceneTimingToConfig(state, { startSecond: 0, endSecond: durationSeconds })

  if (state.config?.plots) {
    const coursePoints = activity?.sample_course_points
    for (const plot of state.config.plots) {
      if (plot.value === 'course' && coursePoints) {
        const dims = getCourseWidgetDimensions(coursePoints, WIDGET_TYPE_DEFINITIONS[plot.value].courseSizing)
        if (dims) {
          plot.width = dims.width
          plot.height = dims.height
        }
      }
    }
  }
}

function createActivitySummary(activity) {
  return {
    availableMetrics: [...activity.valid_attributes, ...activity.extended_attributes].map((attribute) => ({
      attribute,
      source: activity.coverage[attribute].source,
    })),
    durationSeconds: getDurationSeconds(activity),
    endTime: activity.metadata?.end_time ?? null,
    fileFormat: activity.file_format,
    fileName: activity.file_name,
    originalSampleCount: activity.metadata?.original_sample_count ?? null,
    sampleCount: activity.metadata?.sample_count ?? 0,
    sport: activity.metadata?.sport ?? null,
    syncTime: activity.sync_time ?? null,
    timezone: activity.metadata?.timezone ?? null,
    totalDistanceMeters: activity.metadata?.total_distance_m ?? 0,
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
    renderSubmissionTarget: null,
    renderCancellationTarget: null,
    errorMessage: null,
    activitySource: null,
    activitySummary: null,
    parsedActivity: null,
    parsedActivitySource: null, // 'activity-file' | 'video-telemetry' | null
    stashedVideoTelemetry: null,
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
      let summary = null
      if (activity) {
        summary = createActivitySummary(activity)
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

    activateActivityFile: (activity) => {
      setActivityState(set, get, (state) => {
        if (state.parsedActivitySource === 'video-telemetry') {
          state.stashedVideoTelemetry = state.parsedActivity
        }
        state.parsedActivity = activity
        state.parsedActivitySource = 'activity-file'
      })
      get().clearVideoSyncForActivity()
      get().setActivitySummary(activity)
    },

    loadVideoTelemetry: (activity) => {
      const source = get().parsedActivitySource
      if (source === 'activity-file') {
        setActivityState(set, get, (state) => {
          state.stashedVideoTelemetry = activity
        })
      } else {
        get().clearVideoSyncForActivity()
        setActivityState(set, get, (state) => {
          state.parsedActivity = activity
          state.parsedActivitySource = 'video-telemetry'
          state.stashedVideoTelemetry = null
          state.activitySource = null
          state.videoSyncOffsetSeconds = 0
          state.videoSyncOffsetPreviewSeconds = null
          state.videoSyncWarning = null
          state.videoSyncTimezoneMode = null
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
          state.activitySource = null
        }
      })

      if (restoreVideoTelemetry && stashedVideoTelemetry) {
        setActivityState(set, get, (state) => {
          state.parsedActivity = stashedVideoTelemetry
          state.parsedActivitySource = 'video-telemetry'
          state.stashedVideoTelemetry = null
          state.videoSyncOffsetSeconds = 0
          state.videoSyncOffsetPreviewSeconds = null
          state.videoSyncWarning = null
          state.videoSyncTimezoneMode = null
          applyParsedDataToScene(state, stashedVideoTelemetry)
        })
        get().clearVideoSyncForActivity()
        get().setActivitySummary(get().parsedActivity, { computeVideoSync: false })
      } else {
        setActivityState(set, get, (state) => {
          state.parsedActivity = null
          state.parsedActivitySource = null
          state.activitySummary = null
        })
        get().clearVideoSyncForActivity()
      }
    },

    clearVideoTelemetry: () => {
      const { parsedActivitySource } = get()

      setActivityState(set, get, (state) => {
        state.stashedVideoTelemetry = null
        if (parsedActivitySource === 'video-telemetry') {
          state.parsedActivity = null
          state.parsedActivitySource = null
          state.activitySummary = null
        }
      })
      if (parsedActivitySource === 'video-telemetry') {
        get().clearVideoSyncForActivity()
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
          state.videoSyncOffsetPreviewSeconds = null
          state.videoSyncWarning = null
          state.videoSyncTimezoneMode = null
        })
      }
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
