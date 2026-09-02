import { detectCodecs } from '@/api/backend'
import { formatVideoCreationTime, parseVideoFilenameCreationTime } from '@/features/scene-settings/utils/sceneSettingsUtils'
import { createCachedPromise } from '@/lib/cached-promise'
import { videoOverlapsActivity } from '@/lib/video-timing'
import i18next from 'i18next'

/**
 * Converts a timestamp to the comparison clock used for activity sync.
 *
 * Activity/GPS timestamps are absolute UTC instants, so the formatter converts
 * them to the activity timezone before the clock text is parsed. The `ffprobe`
 * path deliberately keeps the tag's clock text unchanged: camera metadata is
 * inconsistent, and a value ending in `Z` may be either real UTC or local
 * camera time incorrectly labeled as UTC. Both interpretations are evaluated
 * in `computeVideoSync` when that source is used.
 */
function parseSyncTimestamp(timestamp, source, timezone) {
  const formattedTimestamp = formatVideoCreationTime(timestamp, source, timezone)
  if (typeof formattedTimestamp !== 'string' || formattedTimestamp.trim() === '') return null

  const normalized = formattedTimestamp.trim().replace(' ', 'T')
  const parsed = Date.parse(normalized.endsWith('Z') ? normalized : `${normalized}Z`)
  return Number.isFinite(parsed) ? parsed : null
}

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

function videoTimestampOverlapsActivity(timestamp, videoDuration, activityStart, activityEnd) {
  return videoOverlapsActivity({
    videoStart: (timestamp - activityStart) / 1000,
    videoDuration,
    activityEnd: (activityEnd - activityStart) / 1000,
  })
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
  videoSyncTimezoneMode: null, // "local" or "utc" when both ffprobe interpretations fit the activity
  availableCodecs: null,
  importedVideoCodecName: null,
  importedVideoCodecLongName: null,
  importedVideoBitRate: null,
  importedVideoCameraType: null,
  importedVideoCameraModel: null,

  setImportedVideo: (metadata) => {
    const importedVideoState = createImportedVideoState(metadata)
    set(importedVideoState)

    get().syncVideoMetadata()

    return importedVideoState.importedVideoResolution
  },

  setImportedBackgroundImage: (path) =>
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
    }),

  clearImportedVideo: () => {
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

  setVideoSyncOffset: (seconds) => {
    validateVideoSyncOffset(seconds, get().importedVideoDuration)
    set({
      videoSyncOffsetSeconds: seconds,
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

  computeVideoSync: (activitySummary) =>
    set((state) => {
      if (!state.importedVideoCreationTime) {
        return {
          videoSyncOffsetSeconds: 0,
          videoSyncWarning: i18next.t('store.couldNotDetermineVideoCreationTime', 'Could not determine video creation time'),
          videoSyncTimezoneMode: null,
        }
      }

      const timezone = activitySummary?.timezone
      if (!timezone) {
        return {
          videoSyncOffsetSeconds: 0,
          videoSyncWarning: i18next.t('store.timezoneIsRequiredForVideoSync', 'timezone is required for video sync'),
          videoSyncTimezoneMode: null,
        }
      }

      const activityStart = parseSyncTimestamp(activitySummary.syncTime, 'gps', timezone)
      const activityEnd = parseSyncTimestamp(activitySummary.endTime, 'gps', timezone)
      if (activityStart === null || activityEnd === null) {
        return {
          videoSyncOffsetSeconds: 0,
          videoSyncWarning: i18next.t('store.invalidTimestampFormats', 'Invalid timestamp formats'),
          videoSyncTimezoneMode: null,
        }
      }

      let videoStart = null
      let timezoneMode = null
      const videoDuration = state.importedVideoDuration

      if (state.importedVideoTimeSource === 'ffprobe' || state.importedVideoTimeSource === 'filename') {
        // An ffprobe `creation_time` tag cannot reliably identify its timezone.
        // Some cameras write a real UTC instant; others write local camera time
        // and append `Z`. Build both candidates, then keep only candidates whose
        // start time lies inside the activity interval. If both survive, the
        // stored mode selects one and the UI exposes the alternate choice.
        const withoutTimezone = parseSyncTimestamp(state.importedVideoCreationTime, 'ffprobe', timezone)
        const withTimezone = parseSyncTimestamp(state.importedVideoCreationTime, 'gps', timezone)
        const candidates = [
          { timestamp: withoutTimezone, timezoneApplied: false },
          { timestamp: withTimezone, timezoneApplied: true },
        ].filter(({ timestamp }) => timestamp !== null && videoTimestampOverlapsActivity(timestamp, videoDuration, activityStart, activityEnd))

        if (withoutTimezone === null || withTimezone === null) {
          return {
            videoSyncOffsetSeconds: 0,
            videoSyncWarning: i18next.t('store.invalidTimestampFormats', 'Invalid timestamp formats'),
            videoSyncTimezoneMode: null,
          }
        }

        if (candidates.length === 0) {
          timezoneMode = state.videoSyncTimezoneMode ?? 'local'
          return {
            videoSyncOffsetSeconds: 0,
            videoSyncWarning: i18next.t('store.videoCouldNotBeSyncedWithActivity', 'Video could not be synced with activity'),
            videoSyncTimezoneMode: timezoneMode,
          }
        }

        const inferredMode = candidates[0].timezoneApplied ? 'utc' : 'local'
        timezoneMode = state.videoSyncTimezoneMode ?? (candidates.length === 2 || inferredMode === 'utc' ? inferredMode : null)
        const selectedMode = timezoneMode ?? inferredMode
        videoStart = selectedMode === 'utc' ? withTimezone : withoutTimezone
      } else {
        videoStart = parseSyncTimestamp(state.importedVideoCreationTime, 'gps', timezone)
      }

      if (videoStart === null) {
        return {
          videoSyncOffsetSeconds: 0,
          videoSyncWarning: i18next.t('store.invalidTimestampFormats', 'Invalid timestamp formats'),
          videoSyncTimezoneMode: null,
        }
      }

      const activityDurationSeconds = (activityEnd - activityStart) / 1000
      const offsetSeconds = Math.min(Math.max((videoStart - activityStart) / 1000, 0), activityDurationSeconds)

      if (!videoTimestampOverlapsActivity(videoStart, videoDuration, activityStart, activityEnd)) {
        return {
          videoSyncOffsetSeconds: 0,
          videoSyncWarning: i18next.t('store.videoCouldNotBeSyncedWithActivity', 'Video could not be synced with activity'),
          videoSyncTimezoneMode: timezoneMode,
        }
      }

      return {
        videoSyncOffsetSeconds: offsetSeconds,
        videoSyncWarning: null,
        videoSyncTimezoneMode: timezoneMode,
      }
    }),
})
