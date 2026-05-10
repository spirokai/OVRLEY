export const createVideoImportSlice = (set) => ({
  importedVideoPath: null, // absolute path from Tauri file dialog
  importedVideoDuration: null, // seconds (float), read via ffprobe
  importedVideoFps: null, // fps (float)
  importedVideoResolution: null, // { width, height }
  importedVideoCreationTime: null, // ISO-8601 string or null
  videoSyncOffsetSeconds: 0, // user-adjustable sync offset
  videoSyncWarning: null, // string warning or null

  setImportedVideo: (metadata) =>
    set({
      importedVideoPath: metadata.path,
      importedVideoDuration: metadata.duration,
      importedVideoFps: metadata.fps,
      importedVideoResolution: metadata.resolution,
      importedVideoCreationTime: metadata.creationTime,
    }),

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
})
