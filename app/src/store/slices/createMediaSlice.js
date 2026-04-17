import { DEFAULT_RENDER_PROGRESS } from '../store-utils'

export function createMediaSlice(set) {
  return {
    generatingImage: false,
    renderingVideo: false,
    errorMessage: null,
    imageFilename: localStorage.getItem('imageFilename') || null,
    videoFilename: localStorage.getItem('videoFilename') || null,
    gpxFilename: localStorage.getItem('gpxFilename') || null,
    renderProgress: { ...DEFAULT_RENDER_PROGRESS },

    setGeneratingImage: (generating) =>
      set((state) => {
        state.generatingImage = generating
      }),

    setRenderingVideo: (rendering) =>
      set((state) => {
        state.renderingVideo = rendering
      }),

    setRenderProgress: (progress) => {
      const percent =
        progress.total > 0
          ? Math.round((progress.current / progress.total) * 100)
          : 0

      set((state) => {
        state.renderProgress = { ...progress, percent }
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
      localStorage.setItem('gpxFilename', filename)
      set((state) => {
        state.gpxFilename = filename
      })

      const isLikelyGpx =
        typeof filename === 'string' &&
        (filename.endsWith('.gpx') || filename.startsWith('http'))
      if (!isLikelyGpx) return

      try {
        const response = await fetch(filename)
        const fileBlob = await response.blob()
        const reader = new FileReader()
        reader.onloadend = () => {}
        reader.readAsDataURL(fileBlob)
      } catch {
        console.warn(
          'setGpxFilename: could not fetch GPX file contents (expected for demo)',
        )
      }
    },

    setGpxFilenameFromFile: (file) => {
      set((state) => {
        state.gpxFilename = file?.name || null
      })
    },
  }
}
