export const createVideoImportSlice = (set, get) => ({
  importedVideoPath: null, // absolute path from Tauri file dialog
  importedVideoDuration: null, // seconds (float), read via ffprobe
  importedVideoFps: null, // fps (float)
  importedVideoResolution: null, // { width, height }
  importedVideoCreationTime: null, // ISO-8601 string or null
  videoSyncOffsetSeconds: 0, // user-adjustable sync offset
  videoSyncWarning: null, // string warning or null

  setImportedVideo: (metadata) => {
    set({
      importedVideoPath: metadata.path,
      importedVideoDuration: metadata.duration,
      importedVideoFps: metadata.fps,
      importedVideoResolution: metadata.resolution,
      importedVideoCreationTime: metadata.creationTime,
    })

    const activitySummary = get().activitySummary
    get().computeVideoSync(activitySummary)
  },

  clearImportedVideo: () =>
    set({
      importedVideoPath: null,
      importedVideoDuration: null,
      importedVideoFps: null,
      importedVideoResolution: null,
      importedVideoCreationTime: null,
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

  computeVideoSync: (activitySummary) =>
    set((state) => {
      if (!state.importedVideoCreationTime) {
        return {
          videoSyncOffsetSeconds: 0,
          videoSyncWarning:
            'Could not determine video creation time  — placed at start',
        }
      }

      const videoStart = new Date(state.importedVideoCreationTime).getTime()
      const activityStart = new Date(activitySummary?.startTime).getTime()
      const activityEnd = new Date(activitySummary?.endTime).getTime()

      if (
        isNaN(videoStart) ||
        (activitySummary && (isNaN(activityStart) || isNaN(activityEnd)))
      ) {
        return {
          videoSyncOffsetSeconds: 0,
          videoSyncWarning: 'Invalid timestamp formats — placed at start',
        }
      }

      const offsetSeconds = (videoStart - activityStart) / 1000

      // within [activityStart, activityEnd]
      if (videoStart < activityStart || videoStart > activityEnd) {
        return {
          videoSyncOffsetSeconds: 0,
          videoSyncWarning:
            'Video creation time is outside activity range — placed at start',
        }
      }

      return {
        videoSyncOffsetSeconds: offsetSeconds,
        videoSyncWarning: null,
      }
    }),
})
