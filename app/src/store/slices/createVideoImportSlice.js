import { detectCodecs } from '@/api/backend'

let availableCodecsPromise = null

export const createVideoImportSlice = (set, get) => ({
  importedVideoPath: null, // absolute path from Tauri file dialog
  importedVideoDuration: null, // seconds (float), read via ffprobe
  importedVideoFps: null, // fps (float)
  importedVideoFpsNum: null, // exact ffprobe FPS numerator
  importedVideoFpsDen: null, // exact ffprobe FPS denominator
  importedVideoResolution: null, // { width, height }
  importedVideoCreationTime: null, // ISO-8601 string or null
  importedVideoImportId: null, // opaque local preview server import ID
  importedVideoPreviewUrl: null, // local HTTP preview URL for the video element
  importedVideoPreviewWarnings: [],
  importedVideoPreviewError: null,
  videoSyncOffsetSeconds: 0, // user-adjustable sync offset
  videoSyncWarning: null, // string warning or null
  availableCodecs: null,

  setImportedVideo: (metadata) => {
    set({
      importedVideoPath: metadata.path,
      importedVideoDuration: metadata.duration,
      importedVideoFps: metadata.fps,
      importedVideoFpsNum: metadata.fpsNum,
      importedVideoFpsDen: metadata.fpsDen,
      importedVideoResolution: metadata.resolution,
      importedVideoCreationTime: metadata.creationTime,
      importedVideoImportId: metadata.importId ?? null,
      importedVideoPreviewUrl: metadata.previewUrl ?? null,
      importedVideoPreviewWarnings: metadata.previewWarnings ?? [],
      importedVideoPreviewError: metadata.previewError ?? null,
    })

    const activitySummary = get().activitySummary
    get().computeVideoSync(activitySummary)
  },

  clearImportedVideo: () =>
    set({
      importedVideoPath: null,
      importedVideoDuration: null,
      importedVideoFps: null,
      importedVideoFpsNum: null,
      importedVideoFpsDen: null,
      importedVideoResolution: null,
      importedVideoCreationTime: null,
      importedVideoImportId: null,
      importedVideoPreviewUrl: null,
      importedVideoPreviewWarnings: [],
      importedVideoPreviewError: null,
      videoSyncOffsetSeconds: 0,
      videoSyncWarning: null,
    }),

  setVideoSyncOffset: (seconds) =>
    set({
      videoSyncOffsetSeconds: seconds,
    }),

  setVideoSyncWarning: (msg) =>
    set({
      videoSyncWarning: msg,
    }),

  setImportedVideoPreviewError: (msg) =>
    set({
      importedVideoPreviewError: msg,
    }),

  setImportedVideoPreviewWarnings: (warnings) =>
    set({
      importedVideoPreviewWarnings: Array.isArray(warnings) ? warnings : [],
    }),

  fetchAvailableCodecs: async () => {
    const cachedCodecs = get().availableCodecs
    if (cachedCodecs) {
      return cachedCodecs
    }

    if (availableCodecsPromise) {
      return availableCodecsPromise
    }

    availableCodecsPromise = detectCodecs()
      .then((availableCodecs) => {
        set({
          availableCodecs,
        })
        return availableCodecs
      })
      .catch((error) => {
        console.error('Failed to detect ffmpeg codecs:', error)
        set({
          availableCodecs: null,
        })
        return null
      })
      .finally(() => {
        availableCodecsPromise = null
      })

    return availableCodecsPromise
  },

  computeVideoSync: (activitySummary) =>
    set((state) => {
      if (!state.importedVideoCreationTime) {
        return {
          videoSyncOffsetSeconds: 0,
          videoSyncWarning: 'Could not determine video creation time  — placed at start',
        }
      }

      const videoStart = new Date(state.importedVideoCreationTime).getTime()
      const activityStart = new Date(activitySummary?.startTime).getTime()
      const activityEnd = new Date(activitySummary?.endTime).getTime()

      if (isNaN(videoStart) || (activitySummary && (isNaN(activityStart) || isNaN(activityEnd)))) {
        return {
          videoSyncOffsetSeconds: 0,
          videoSyncWarning: 'Invalid timestamp formats',
        }
      }

      const offsetSeconds = (videoStart - activityStart) / 1000

      // within [activityStart, activityEnd]
      if (videoStart < activityStart || videoStart > activityEnd) {
        return {
          videoSyncOffsetSeconds: 0,
          videoSyncWarning: 'Video creation time is outside activity range',
        }
      }

      return {
        videoSyncOffsetSeconds: offsetSeconds,
        videoSyncWarning: null,
      }
    }),
})
