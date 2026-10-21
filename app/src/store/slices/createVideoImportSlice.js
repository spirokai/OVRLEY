import { detectCodecs } from '@/api/backend'
import { parseVideoFilenameCreationTime } from '@/features/scene-settings/utils/sceneSettingsUtils'
import { getTimelineMinimum, getTotalPlaybackDuration } from '@/features/player/utils/playerTiming'
import { createCachedPromise } from '@/lib/cached-promise'
import { clamp } from '@/lib/utils'
import { frameCountForDuration } from '@/lib/video-program/frame-grid'
import { DEFAULT_TRANSITION_SETTINGS, NO_TRANSITION, XFADE_TRANSITIONS } from '@/lib/video-program/transition-catalog'
import { getVideosTimelineBounds, joinVideoEdits, sortVideosByTimeline } from '@/lib/video-program/video-collection'
import { placeVideosByCreationTime, resolveVideosSync } from '@/lib/video-sync'
import { applyCoursePlotDimensions } from '../store-utils'

let fetchCodecsOnce = null

const TELEMETRY_STATUSES = new Set(['pending', 'ready', 'absent', 'failed'])

function requirePositiveInteger(value, label) {
  if (!Number.isInteger(value) || value <= 0) throw new Error(`Imported video metadata contains an invalid ${label}`)
}

function greatestCommonDivisor(left, right) {
  return right === 0 ? left : greatestCommonDivisor(right, left % right)
}

function displayResolutionForImportedVideo(metadata) {
  const resolution = metadata.resolution
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

function requireTelemetry({ status, activity }) {
  if (!TELEMETRY_STATUSES.has(status)) throw new Error(`Invalid video telemetry status: ${status}`)
  if ((status === 'ready') !== (activity !== null)) throw new Error('Only ready video telemetry carries an activity')
}

function createVideoId() {
  return globalThis.crypto.randomUUID()
}

/**
 * Validates probed source metadata once at import ingress and creates a canonical video entity.
 * Duration is stored on the exact frame grid: `frameCount × fps.den / fps.num`. User edits
 * (placement, deleted parts, transitions) live in `videoEdits` keyed by the entity ID.
 * @param {object} metadata Probed source metadata.
 * @param {object} options Entity identity.
 * @param {number} options.importOrder Monotonic import order within the session.
 * @param {{importId: string, url: string, warnings: Array<string>}} options.preview Local preview registration.
 * @param {{status: string, activity: object|null}} [options.telemetry] Settled embedded telemetry; pending when absent.
 * @param {string} [options.id] Stable entity ID; generated when absent.
 * @returns {object} Imported video entity.
 */
export function createImportedVideo(metadata, { importOrder, preview, telemetry = { status: 'pending', activity: null }, id = createVideoId() }) {
  if (typeof metadata.path !== 'string' || metadata.path.length === 0) {
    throw new Error('Imported video metadata must include a path')
  }
  if (!Number.isFinite(metadata.duration) || metadata.duration <= 0) {
    throw new Error('Imported video metadata contains an invalid duration')
  }
  requirePositiveInteger(metadata.fpsNum, 'frame rate numerator')
  requirePositiveInteger(metadata.fpsDen, 'frame rate denominator')
  if (typeof metadata.hasAudio !== 'boolean') {
    throw new Error('Imported video metadata must report audio presence')
  }
  if (typeof preview.importId !== 'string' || typeof preview.url !== 'string' || !Array.isArray(preview.warnings)) {
    throw new Error('Imported video requires a preview registration')
  }
  requireTelemetry(telemetry)

  const divisor = greatestCommonDivisor(metadata.fpsNum, metadata.fpsDen)
  const fps = { num: metadata.fpsNum / divisor, den: metadata.fpsDen / divisor }
  // A positive duration always has at least one frame.
  const frameCount = frameCountForDuration(metadata.duration, fps)

  return {
    id,
    path: metadata.path,
    importOrder,
    durationSeconds: (frameCount * fps.den) / fps.num,
    frameCount,
    fps,
    resolution: displayResolutionForImportedVideo(metadata),
    hasAudio: metadata.hasAudio,
    codecName: metadata.codecName ?? null,
    codecLongName: metadata.codecLongName ?? null,
    bitRate: metadata.bitRate ?? null,
    cameraType: metadata.cameraType ?? null,
    cameraModel: metadata.cameraModel ?? null,
    creationTime: metadata.creationTime ?? null,
    timeSource: metadata.timeSource ?? null,
    detectedCreationTime: metadata.creationTime ?? null,
    detectedTimeSource: metadata.timeSource ?? null,
    preview: { importId: preview.importId, url: preview.url, warnings: [...preview.warnings] },
    syncWarning: null,
    telemetry: { status: telemetry.status, activity: telemetry.activity },
  }
}

/**
 * Creates the user edits of a newly imported video. `placementAnchor` places a
 * clip whose time cannot be determined directly before or after another clip.
 * @returns {{offsetSeconds: number, deletedParts: Array<object>, transitionIn: object|null, placementAnchor: {videoId: string, side: 'before'|'after'}|null}} Initial edits.
 */
export function createVideoEdits() {
  return { offsetSeconds: 0, deletedParts: [], transitionIn: null, placementAnchor: null }
}

/**
 * Returns the timeline start that puts a clip directly before or after its anchor clip.
 * @param {{durationSeconds: number}} video Placed clip.
 * @param {{offsetSeconds: number, durationSeconds: number}} anchor Anchor clip with its committed offset.
 * @param {'before'|'after'} side Side of the anchor.
 * @returns {number} Timeline start second.
 */
export function anchoredOffsetSeconds(video, anchor, side) {
  return side === 'before' ? anchor.offsetSeconds - video.durationSeconds : anchor.offsetSeconds + anchor.durationSeconds
}

/**
 * Returns the next import order for videos added to an existing collection.
 * @param {Array<object>} videos Current imported videos.
 * @returns {number} Next monotonic import order.
 */
export function nextImportOrder(videos) {
  return videos.reduce((next, video) => Math.max(next, video.importOrder + 1), 0)
}

function requireVideo(state, videoId) {
  if (!state.importedVideos.some((candidate) => candidate.id === videoId)) throw new Error(`Unknown imported video: ${videoId}`)
}

function requireFiniteOffset(seconds, label) {
  if (!Number.isFinite(seconds)) throw new Error(`${label} must be a finite number`)
}

/**
 * Detects the placement of every given clip: against the external activity when
 * one is loaded, otherwise at detected real time relative to the earliest clip.
 * @returns {{placements: Array<{offsetSeconds: number, determined: boolean, syncWarning: string|null}>, timezoneMode: 'utc'|'local'|null}}
 *   Placements in input order and the resulting session timezone mode.
 */
function detectVideoPlacements(state, videos) {
  if (state.parsedActivity === null) return { placements: placeVideosByCreationTime(videos), timezoneMode: null }
  return resolveVideosSync(videos, state.activitySummary, state.videoSyncTimezoneMode)
}

/**
 * Commits detected placements. A clip whose time cannot be determined but that has
 * a placement anchor sits next to its anchor instead; anchors are resolved after
 * the clips they point to (anchors never form cycles).
 */
function applyPlacements(draft, placements) {
  const anchored = []
  draft.importedVideos.forEach((video, index) => {
    const edits = draft.videoEdits[video.id]
    if (!placements[index].determined && edits.placementAnchor !== null) {
      anchored.push(video)
      return
    }
    edits.offsetSeconds = placements[index].offsetSeconds
    video.syncWarning = placements[index].syncWarning
  })
  const resolved = new Set(draft.importedVideos.filter((video) => !anchored.includes(video)).map((video) => video.id))
  while (anchored.length > 0) {
    const index = anchored.findIndex((video) => resolved.has(draft.videoEdits[video.id].placementAnchor.videoId))
    if (index === -1) throw new Error('Video placement anchors must reference imported videos')
    const [video] = anchored.splice(index, 1)
    const { videoId, side } = draft.videoEdits[video.id].placementAnchor
    const anchor = draft.importedVideos.find((candidate) => candidate.id === videoId)
    draft.videoEdits[video.id].offsetSeconds = anchoredOffsetSeconds(video, { ...anchor, ...draft.videoEdits[videoId] }, side)
    video.syncWarning = null
    resolved.add(video.id)
  }
}

function requireTransitionDefaults({ type, durationSeconds, splitGapThresholdSeconds }) {
  if (type !== NO_TRANSITION && !XFADE_TRANSITIONS.includes(type)) throw new Error(`Unknown transition type: ${type}`)
  if (!Number.isFinite(durationSeconds) || durationSeconds <= 0) throw new Error('Transition duration must be a positive number of seconds')
  if (!Number.isFinite(splitGapThresholdSeconds) || splitGapThresholdSeconds < 0) {
    throw new Error('Continuity threshold must be zero or a positive number of seconds')
  }
}

function clampPlayheadToVideos(draft, deltaSeconds) {
  const bounds = getVideosTimelineBounds(joinVideoEdits(draft.importedVideos, draft.videoEdits))
  const timelineMinimum = getTimelineMinimum({ videoBounds: bounds })
  const totalDuration = getTotalPlaybackDuration({
    activityDurationSeconds: draft.activitySummary?.durationSeconds,
    fallbackDurationSeconds: draft.fallbackDurationSeconds,
    videoBounds: bounds,
  })
  draft.selectedSecond = clamp(draft.selectedSecond + deltaSeconds, timelineMinimum, totalDuration)
}

export const createVideoImportSlice = (set, get) => ({
  importedVideos: [], // canonical imported-video entities (see createImportedVideo)
  videoEdits: {}, // undoable user edits keyed by video ID (see createVideoEdits)
  importedBackgroundImagePath: null, // absolute path from Tauri file dialog
  videoDragPreview: null, // { videoId: string|null, deltaSeconds } while one clip (or, with null, every clip) is dragged
  videoSyncTimezoneMode: null, // session "Apply Timezone" selection: "local" | "utc"; null requests automatic inference
  transitionDefaults: { ...DEFAULT_TRANSITION_SETTINGS },
  availableCodecs: null,

  /** Replaces the collection with already-registered video entities and places them. */
  setImportedVideos: (videos) => {
    set({
      importedVideos: videos,
      videoEdits: Object.fromEntries(videos.map((video) => [video.id, createVideoEdits()])),
      importedBackgroundImagePath: null,
      videoDragPreview: null,
      deletedPartMark: null,
    })
    get().computeVideoSync()
    get().selectVideoSyncVideo(videos.length > 0 ? videos[0].id : null)
  },

  setImportedBackgroundImage: (path) => {
    set({
      importedVideos: [],
      videoEdits: {},
      importedBackgroundImagePath: path || null,
      videoDragPreview: null,
      videoSyncTimezoneMode: null,
      deletedPartMark: null,
    })
    get().selectVideoSyncVideo(null)
  },

  clearImportedVideos: () => {
    set({
      importedVideos: [],
      videoEdits: {},
      importedBackgroundImagePath: null,
      videoDragPreview: null,
      videoSyncTimezoneMode: null,
      deletedPartMark: null,
    })
    get().selectVideoSyncVideo(null)
  },

  /**
   * Adds already-registered videos to the collection. Existing clips keep their
   * committed placement. New clips are placed by detected time plus the shared
   * correction of the collection: committed minus detected offset of the earliest
   * existing clip whose time can be determined (zero without one).
   * @param {Array<object>} videos Imported video entities with paths not yet in the collection.
   * @returns {Array<string>} IDs of added clips whose time could not be determined, in input order.
   */
  addImportedVideos: (videos) => {
    const state = get()
    if (videos.length === 0) throw new Error('At least one video is required')
    const paths = new Set(state.importedVideos.map((video) => video.path))
    for (const video of videos) {
      if (paths.has(video.path)) throw new Error(`Video is already imported: ${video.path}`)
      paths.add(video.path)
    }

    const existingCount = state.importedVideos.length
    const { placements, timezoneMode } = detectVideoPlacements(state, [...state.importedVideos, ...videos])
    const placementsById = new Map([...state.importedVideos, ...videos].map((video, index) => [video.id, placements[index]]))
    const reference = sortVideosByTimeline(joinVideoEdits(state.importedVideos, state.videoEdits)).find(
      (clip) => placementsById.get(clip.id).determined,
    )
    const correctionSeconds = reference ? reference.offsetSeconds - placementsById.get(reference.id).offsetSeconds : 0

    set((draft) => {
      for (const video of videos) {
        const placement = placementsById.get(video.id)
        draft.importedVideos.push({ ...video, syncWarning: placement.syncWarning })
        draft.videoEdits[video.id] = {
          ...createVideoEdits(),
          offsetSeconds: placement.determined ? placement.offsetSeconds + correctionSeconds : placement.offsetSeconds,
        }
      }
      draft.importedBackgroundImagePath = null
      draft.videoDragPreview = null
      draft.videoSyncTimezoneMode = timezoneMode
    })
    if (existingCount === 0) get().selectVideoSyncVideo(videos[0].id)
    return videos.filter((video) => !placementsById.get(video.id).determined).map((video) => video.id)
  },

  /**
   * Places a clip directly before or after another clip and keeps that placement
   * for as long as the clip's own time cannot be determined.
   */
  anchorVideo: (videoId, anchorVideoId, side) => {
    const state = get()
    requireVideo(state, videoId)
    requireVideo(state, anchorVideoId)
    if (side !== 'before' && side !== 'after') throw new Error('Placement side must be before or after')
    for (let current = anchorVideoId; current !== null; current = state.videoEdits[current].placementAnchor?.videoId ?? null) {
      if (current === videoId) throw new Error('A video cannot be placed relative to itself')
    }
    set((draft) => {
      const video = draft.importedVideos.find((candidate) => candidate.id === videoId)
      draft.videoEdits[videoId].placementAnchor = { videoId: anchorVideoId, side }
      video.syncWarning = null
      draft.videoDragPreview = null
      // The placed clip moves next to its anchor, and clips anchored to it follow.
      const moved = [videoId]
      while (moved.length > 0) {
        const movedId = moved.shift()
        const { videoId: currentAnchorId, side: currentSide } = draft.videoEdits[movedId].placementAnchor
        const movedVideo = draft.importedVideos.find((candidate) => candidate.id === movedId)
        const anchor = draft.importedVideos.find((candidate) => candidate.id === currentAnchorId)
        draft.videoEdits[movedId].offsetSeconds = anchoredOffsetSeconds(movedVideo, { ...anchor, ...draft.videoEdits[currentAnchorId] }, currentSide)
        moved.push(
          ...draft.importedVideos
            .filter((candidate) => draft.videoEdits[candidate.id].placementAnchor?.videoId === movedId)
            .map((candidate) => candidate.id),
        )
      }
    })
  },

  /**
   * Removes one clip with its edits. Clips anchored to it inherit its anchor (or
   * keep their position when it had none), manual sync moves to the first
   * remaining clip when it targeted the removed one, and the playhead stays on
   * the timeline. Compatibility, overlaps, and joins are derived and follow.
   */
  removeImportedVideo: (videoId) => {
    const state = get()
    requireVideo(state, videoId)
    const removedAnchor = state.videoEdits[videoId].placementAnchor
    set((draft) => {
      draft.importedVideos = draft.importedVideos.filter((video) => video.id !== videoId)
      delete draft.videoEdits[videoId]
      for (const edits of Object.values(draft.videoEdits)) {
        if (edits.placementAnchor?.videoId === videoId) edits.placementAnchor = removedAnchor === null ? null : { ...removedAnchor }
      }
      draft.videoDragPreview = null
      if (draft.deletedPartMark?.videoId === videoId) draft.deletedPartMark = null
      clampPlayheadToVideos(draft, 0)
    })
    if (get().manualVideoSync.videoId === videoId) {
      const [first] = sortVideosByTimeline(joinVideoEdits(get().importedVideos, get().videoEdits))
      get().selectVideoSyncVideo(first?.id ?? null)
    }
  },

  /** Stores the project transition defaults chosen by the user. */
  setTransitionDefaults: (defaults) => {
    requireTransitionDefaults(defaults)
    const { type, durationSeconds, splitGapThresholdSeconds } = defaults
    set({ transitionDefaults: { type, durationSeconds, splitGapThresholdSeconds } })
  },

  /** Places every clip by detected time (see `detectVideoPlacements`). */
  computeVideoSync: () => {
    const state = get()
    if (state.importedVideos.length === 0) return
    const { placements, timezoneMode } = detectVideoPlacements(state, state.importedVideos)
    set((draft) => {
      applyPlacements(draft, placements)
      draft.videoSyncTimezoneMode = timezoneMode
    })
  },

  /** Sets one clip's committed timeline start. */
  setVideoOffset: (videoId, seconds, { compensatePlayhead = false } = {}) => {
    requireFiniteOffset(seconds, 'Video offset')
    requireVideo(get(), videoId)
    const deltaSeconds = seconds - get().videoEdits[videoId].offsetSeconds
    set((draft) => {
      draft.videoEdits[videoId].offsetSeconds = seconds
      draft.importedVideos.find((candidate) => candidate.id === videoId).syncWarning = null
      draft.videoDragPreview = null
      if (compensatePlayhead) clampPlayheadToVideos(draft, deltaSeconds)
    })
  },

  /** Shifts every clip by the same amount, keeping each clip's individual adjustment. */
  shiftVideoOffsets: (deltaSeconds, { compensatePlayhead = false } = {}) => {
    requireFiniteOffset(deltaSeconds, 'Video offset shift')
    if (get().importedVideos.length === 0) throw new Error('A video must be loaded before shifting video offsets')
    set((draft) => {
      for (const video of draft.importedVideos) {
        draft.videoEdits[video.id].offsetSeconds += deltaSeconds
        video.syncWarning = null
      }
      draft.videoDragPreview = null
      if (compensatePlayhead) clampPlayheadToVideos(draft, deltaSeconds)
    })
  },

  /** Sets or clears the transient shift of the dragged clip, or of every clip when `videoId` is null. */
  setVideoDragPreview: (dragPreview) => {
    if (dragPreview !== null) {
      if (dragPreview.videoId !== null) requireVideo(get(), dragPreview.videoId)
      requireFiniteOffset(dragPreview.deltaSeconds, 'Video drag shift')
    }
    set({ videoDragPreview: dragPreview })
  },

  setVideoSyncTimezoneMode: (mode) => {
    if (mode !== 'local' && mode !== 'utc') {
      throw new Error('Video sync timezone mode must be local or utc')
    }

    set({ videoSyncTimezoneMode: mode })
    get().computeVideoSync()
  },

  /** Uses the `YYYYMMDD_HHMMSS` filename timestamp of every clip that has one. */
  setVideoCreationTimesFromFilenames: () => {
    const filenameTimes = get().importedVideos.map((video) => parseVideoFilenameCreationTime(video.path))
    if (filenameTimes.every((creationTime) => creationTime === null)) {
      throw new Error('No video filename contains a valid YYYYMMDD_HHMMSS timestamp')
    }

    set((draft) => {
      draft.importedVideos.forEach((video, index) => {
        if (filenameTimes[index] === null) return
        video.creationTime = filenameTimes[index]
        video.timeSource = 'filename'
      })
      draft.videoSyncTimezoneMode = 'local'
    })
    get().computeVideoSync()
  },

  resetVideoCreationTimes: () => {
    set((draft) => {
      for (const video of draft.importedVideos) {
        video.creationTime = video.detectedCreationTime
        video.timeSource = video.detectedTimeSource
      }
      draft.videoSyncTimezoneMode = null
    })
    get().computeVideoSync()
  },

  /**
   * Stores the settled embedded-telemetry result of one clip. Results for clips
   * removed in the meantime are discarded.
   * @returns {boolean} Whether the result was stored.
   */
  setVideoTelemetry: (videoId, { status, activity }) => {
    if (status === 'pending') throw new Error('Video telemetry results must be settled')
    requireTelemetry({ status, activity })
    if (!get().importedVideos.some((video) => video.id === videoId)) return false
    set((draft) => {
      draft.importedVideos.find((video) => video.id === videoId).telemetry = { status, activity }
      if (status === 'ready' && draft.parsedActivity === null) applyCoursePlotDimensions(draft, activity.sample_course_points)
    })
    return true
  },

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
})
