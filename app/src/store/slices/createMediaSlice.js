import { DEFAULT_RENDER_PROGRESS } from '../store-utils'

export function createMediaSlice(set) {
  localStorage.removeItem('gpxFilename')

  return {
    generatingImage: false,
    renderingVideo: false,
    errorMessage: null,
    imageFilename: localStorage.getItem('imageFilename') || null,
    videoFilename: localStorage.getItem('videoFilename') || null,
    gpxFilename: null,
    activitySummary: null,
    activeRenderId: null,
    renderProgress: { ...DEFAULT_RENDER_PROGRESS },

    setGeneratingImage: (generating) =>
      set((state) => {
        state.generatingImage = generating
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
      const percent =
        progress.total > 0
          ? Math.round((progress.current / progress.total) * 100)
          : 0

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

    setImageFilename: (filename) => {
      localStorage.setItem('imageFilename', filename)
      set((state) => {
        state.imageFilename = filename
      })
    },

    setVideoFilename: (filename) => {
      localStorage.setItem('videoFilename', filename)
      set((state) => {
        state.videoFilename = filename
      })
    },

    setGpxFilename: async (filename) => {
      set((state) => {
        state.gpxFilename = filename
      })
    },

    setGpxFilenameFromFile: (file) => {
      set((state) => {
        state.gpxFilename = file?.name || null
      })
    },

    setActivitySummary: (activity) => {
      set((state) => {
        if (!activity) {
          state.activitySummary = null
          return
        }

        state.activitySummary = {
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
      })
    },

    clearActivitySummary: () => {
      set((state) => {
        state.activitySummary = null
      })
    },
  }
}
