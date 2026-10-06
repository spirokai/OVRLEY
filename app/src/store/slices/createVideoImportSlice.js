import { detectCodecs } from '@/api/backend'
import { parseVideoFilenameCreationTime } from '@/features/scene-settings/utils/sceneSettingsUtils'
import { resolveVideoSyncState } from '@/lib/video-sync'
import { createCachedPromise } from '@/lib/cached-promise'
import { videoOverlapsActivity } from '@/lib/video-timing'
import { clamp } from '@/lib/utils'
import { getTimelineMinimum, getTotalPlaybackDuration } from '@/features/player/utils/playerTiming'

let fetchCodecsOnce = null

function displayResolutionForImportedVideo(metadata) {
  const resolution = metadata?.resolution
  if (!resolution) {
    throw new Error('Imported video metadata must include a resolution')
  }

  const width = Number(resolution.width)
  const height = Number(resolution.height)
  if (!Number.isFinite(width) || !Number.isFinite(height) || width <= 0 || height <= 0) {
    throw new Error('Imported video metadata contains an invalid resolution')
  }

  const rotation = metadata.rotationDegrees === null || metadata.rotationDegrees === undefined ? 0 : Number(metadata.rotationDegrees)
  if (!Number.isFinite(rotation)) {
    throw new Error('Imported video metadata contains an invalid rotation')
  }

  const normalizedRotation = ((rotation % 360) + 360) % 360
  if (![0, 90, 180, 270].includes(normalizedRotation)) {
    throw new Error('Imported video metadata contains an unsupported rotation')
  }

  if (normalizedRotation === 90 || normalizedRotation === 270) {
    return { width: height, height: width }
  }

  return { width, height }
}

function validateImportedVideoTiming(metadata) {
  if (typeof metadata.path !== 'string' || metadata.path.length === 0) {
    throw new Error('Imported video metadata must include a path')
  }
  if (!Number.isFinite(metadata.duration) || metadata.duration <= 0) {
    throw new Error('Imported video metadata contains an invalid duration')
  }
  if (!Number.isFinite(metadata.fps) || metadata.fps <= 0) {
    throw new Error('Imported video metadata contains an invalid frame rate')
  }
}

export function createImportedVideoState(metadata) {
  validateImportedVideoTiming(metadata)
  return {
    importedVideoPath: metadata.path,
    importedVideoDuration: metadata.duration,
    importedVideoFps: metadata.fps,
    importedVideoFpsNum: metadata.fpsNum,
    importedVideoFpsDen: metadata.fpsDen,
    importedVideoResolution: displayResolutionForImportedVideo(metadata),
    importedVideoCreationTime: metadata.creationTime,
    importedVideoTimeSource: metadata.timeSource ?? null,
    detectedVideoCreationTime: metadata.creationTime,
    detectedVideoTimeSource: metadata.timeSource ?? null,
    importedVideoImportId: metadata.importId ?? null,
    importedVideoPreviewUrl: metadata.previewUrl ?? null,
    importedVideoPreviewWarnings: metadata.previewWarnings ?? [],
    importedBackgroundImagePath: null,
    videoSyncOffsetPreviewSeconds: null,
    videoSyncTimezoneMode: null,
    importedVideoCodecName: metadata.codecName ?? null,
    importedVideoCodecLongName: metadata.codecLongName ?? null,
    importedVideoBitRate: metadata.bitRate ?? null,
    importedVideoCameraType: metadata.cameraType ?? null,
    importedVideoCameraModel: metadata.cameraModel ?? null,
  }
}

function validateVideoSyncOffset(seconds, videoDuration, label = 'Video sync offset') {
  if (!Number.isFinite(seconds)) {
    throw new Error(`${label} must be a finite number`)
  }
  if (videoDuration === null) return
  if (!videoOverlapsActivity({ videoStart: seconds, videoDuration })) {
    throw new Error(`${label} must leave a positive overlap with the imported video`)
  }
}

export const createVideoImportSlice = (set, get) => ({
  importedVideoPath: null, // absolute path from Tauri file dialog
  importedVideoDuration: null, // seconds (float), read via ffprobe
  importedVideoFps: null, // fps (float)
  importedVideoFpsNum: null, // exact ffprobe FPS numerator
  importedVideoFpsDen: null, // exact ffprobe FPS denominator
  importedVideoResolution: null, // display-oriented { width, height }
  importedVideoCreationTime: null, // ISO-8601 string or null
  importedVideoTimeSource: null, // "gps" | "ffprobe" | "file_mtime" | "filename" | null
  detectedVideoCreationTime: null, // original creation time from imported metadata
  detectedVideoTimeSource: null, // original creation-time source from imported metadata
  importedVideoImportId: null, // opaque local preview server import ID
  importedVideoPreviewUrl: null, // local HTTP preview URL for the video element
  importedVideoPreviewWarnings: [],
  importedBackgroundImagePath: null, // absolute path from Tauri file dialog
  videoSyncOffsetSeconds: 0, // user-adjustable sync offset
  videoSyncOffsetPreviewSeconds: null, // transient drag preview; committed on release
  videoSyncWarning: null, // string warning or null
  videoSyncTimezoneMode: null, // "local" or "utc" for camera timestamps; null is initially unchecked
  availableCodecs: null,
  importedVideoCodecName: null,
  importedVideoCodecLongName: null,
  importedVideoBitRate: null,
  importedVideoCameraType: null,
  importedVideoCameraModel: null,

  setImportedVideo: (metadata) => {
    const importedVideoState = createImportedVideoState(metadata)
    get().clearVideoSyncForVideo()
    set(importedVideoState)

    get().syncVideoMetadata()

    return importedVideoState.importedVideoResolution
  },

  setImportedBackgroundImage: (path) => {
    get().clearVideoSyncForVideo()
    get().clearVideoTelemetry()
    set({
      importedVideoPath: null,
      importedVideoDuration: null,
      importedVideoFps: null,
      importedVideoFpsNum: null,
      importedVideoFpsDen: null,
      importedVideoResolution: null,
      importedVideoCreationTime: null,
      importedVideoTimeSource: null,
      detectedVideoCreationTime: null,
      detectedVideoTimeSource: null,
      importedVideoImportId: null,
      importedVideoPreviewUrl: null,
      importedVideoPreviewWarnings: [],
      importedBackgroundImagePath: path || null,
      videoSyncOffsetSeconds: 0,
      videoSyncOffsetPreviewSeconds: null,
      videoSyncWarning: null,
      videoSyncTimezoneMode: null,
      importedVideoCodecName: null,
      importedVideoCodecLongName: null,
      importedVideoBitRate: null,
      importedVideoCameraType: null,
      importedVideoCameraModel: null,
    })
  },

  clearImportedVideo: () => {
    get().clearVideoSyncForVideo()
    get().clearVideoTelemetry()
    set({
      importedVideoPath: null,
      importedVideoDuration: null,
      importedVideoFps: null,
      importedVideoFpsNum: null,
      importedVideoFpsDen: null,
      importedVideoResolution: null,
      importedVideoCreationTime: null,
      importedVideoTimeSource: null,
      detectedVideoCreationTime: null,
      detectedVideoTimeSource: null,
      importedVideoImportId: null,
      importedVideoPreviewUrl: null,
      importedVideoPreviewWarnings: [],
      importedBackgroundImagePath: null,
      videoSyncOffsetSeconds: 0,
      videoSyncOffsetPreviewSeconds: null,
      videoSyncWarning: null,
      videoSyncTimezoneMode: null,
      importedVideoCodecName: null,
      importedVideoCodecLongName: null,
      importedVideoBitRate: null,
      importedVideoCameraType: null,
      importedVideoCameraModel: null,
    })
  },

  setVideoSyncOffset: (seconds, { compensatePlayhead = false } = {}) => {
    validateVideoSyncOffset(seconds, get().importedVideoDuration)
    if (!compensatePlayhead) {
      set({
        videoSyncOffsetSeconds: seconds,
        videoSyncWarning: null,
      })
      return
    }

    const state = get()
    const offsetDelta = seconds - state.videoSyncOffsetSeconds
    const timelineMinimum = getTimelineMinimum({
      hasVideo: state.importedVideoPath !== null,
      videoSyncOffsetSeconds: seconds,
    })
    const totalDuration = getTotalPlaybackDuration({
      activityDurationSeconds: state.activitySummary?.durationSeconds,
      fallbackDurationSeconds: state.fallbackDurationSeconds,
      importedVideoDuration: state.importedVideoDuration,
      importedVideoPath: state.importedVideoPath,
      videoSyncOffsetSeconds: seconds,
    })

    set((draft) => {
      draft.videoSyncOffsetSeconds = seconds
      draft.videoSyncWarning = null
      draft.selectedSecond = clamp(state.selectedSecond + offsetDelta, timelineMinimum, totalDuration)
      draft.videoSyncOffsetPreviewSeconds = null
    })
  },

  setVideoSyncOffsetPreview: (seconds) => {
    if (seconds !== null) validateVideoSyncOffset(seconds, get().importedVideoDuration, 'Video sync offset preview')
    set({
      videoSyncOffsetPreviewSeconds: seconds,
    })
  },

  setVideoSyncWarning: (msg) =>
    set({
      videoSyncWarning: msg,
    }),

  setVideoSyncTimezoneMode: (mode) => {
    if (mode !== 'local' && mode !== 'utc') {
      throw new Error('Video sync timezone mode must be local or utc')
    }

    set({ videoSyncTimezoneMode: mode })
    get().computeVideoSync(get().activitySummary)
  },

  setVideoCreationTimeFromFilename: () => {
    const creationTime = parseVideoFilenameCreationTime(get().importedVideoPath)
    if (creationTime === null) {
      throw new Error('Video filename does not contain a valid YYYYMMDD_HHMMSS timestamp')
    }

    set({
      importedVideoCreationTime: creationTime,
      importedVideoTimeSource: 'filename',
      videoSyncTimezoneMode: 'local',
    })
    get().computeVideoSync(get().activitySummary)
  },

  resetVideoCreationTime: () => {
    const state = get()
    set({
      importedVideoCreationTime: state.detectedVideoCreationTime,
      importedVideoTimeSource: state.detectedVideoTimeSource,
      videoSyncTimezoneMode: null,
    })
    get().computeVideoSync(get().activitySummary)
  },

  setImportedVideoPreviewWarnings: (warnings) =>
    set({
      importedVideoPreviewWarnings: Array.isArray(warnings) ? warnings : [],
    }),

  fetchAvailableCodecs: async () => {
    const cachedCodecs = get().availableCodecs
    if (cachedCodecs) {
      return cachedCodecs
    }

    if (!fetchCodecsOnce) {
      fetchCodecsOnce = createCachedPromise(detectCodecs)
    }

    try {
      const availableCodecs = await fetchCodecsOnce()
      set({ availableCodecs })
      return availableCodecs
    } catch (error) {
      console.error('Failed to detect ffmpeg codecs:', error)
      set({ availableCodecs: null })
      return null
    }
  },

  computeVideoSync: (activitySummary) => set((state) => resolveVideoSyncState(state, activitySummary)),
})
