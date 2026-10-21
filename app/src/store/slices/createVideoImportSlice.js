import { detectCodecs } from '@/api/backend'
import { getTimelineMinimum, getTotalPlaybackDuration } from '@/features/player/utils/playerTiming'
import { createCachedPromise } from '@/lib/cached-promise'
import { clamp } from '@/lib/utils'
import {
  placeAutomaticVideos,
  removeVideo,
  resetCreationTimes,
  resetVideoOffset,
  setVideoOffset,
  setVideoSyncTimezoneMode,
  shiftVideoOffsets,
  applyFilenameCreationTimes,
} from '@/lib/video-program/collection-edits'
import { requireTelemetry } from '@/lib/video-program/imported-video'
import { DEFAULT_TRANSITION_SETTINGS, NO_TRANSITION, XFADE_TRANSITIONS } from '@/lib/video-program/transition-catalog'
import { getVideosTimelineBounds, joinVideoEdits, sortVideosByTimeline } from '@/lib/video-program/video-collection'
import { applyCoursePlotDimensions } from '../store-utils'

let fetchCodecsOnce = null

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

  /**
   * Replaces the collection with registered video entities and their edits (a reviewed
   * import or a loaded project). A manual-sync selection of a video no longer in the
   * collection moves to the first clip; the playhead stays on the timeline.
   * @param {{importedVideos: Array<object>, videoEdits: object, videoSyncTimezoneMode: 'utc'|'local'|null}} collection Collection to commit.
   */
  commitVideoCollection: ({ importedVideos, videoEdits, videoSyncTimezoneMode }) => {
    if (importedVideos.some((video) => video.preview === null)) throw new Error('Committed videos must be registered for preview')
    set((draft) => {
      draft.importedVideos = importedVideos
      draft.videoEdits = videoEdits
      draft.videoSyncTimezoneMode = videoSyncTimezoneMode
      draft.importedBackgroundImagePath = null
      draft.videoDragPreview = null
      if (!importedVideos.some((video) => video.id === draft.deletedPartMark?.videoId)) draft.deletedPartMark = null
      clampPlayheadToVideos(draft, 0)
    })
    const { manualVideoSync } = get()
    if (!importedVideos.some((video) => video.id === manualVideoSync.videoId)) {
      const [first] = sortVideosByTimeline(joinVideoEdits(importedVideos, videoEdits))
      get().selectVideoSyncVideo(first?.id ?? null)
    }
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
   * Removes one clip with its edits (see `removeVideo`). Manual sync moves to the
   * first remaining clip when it targeted the removed one, and the playhead stays
   * on the timeline. Compatibility, overlaps, and joins are derived and follow.
   */
  removeImportedVideo: (videoId) => {
    set((draft) => {
      removeVideo(draft, videoId)
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

  /** Places every automatic clip by detected time (see `placeAutomaticVideos`). */
  computeVideoSync: () => {
    set((draft) => placeAutomaticVideos(draft))
  },

  /** Sets one clip's start by hand (see `setVideoOffset`). */
  setVideoOffset: (videoId, seconds, { compensatePlayhead = false } = {}) => {
    set((draft) => {
      const deltaSeconds = seconds - (draft.videoEdits[videoId]?.offsetSeconds ?? 0)
      setVideoOffset(draft, videoId, seconds)
      draft.videoDragPreview = null
      if (compensatePlayhead) clampPlayheadToVideos(draft, deltaSeconds)
    })
  },

  /** Returns a clip set by hand to automatic placement (see `resetVideoOffset`). */
  resetVideoOffset: (videoId) => {
    set((draft) => {
      resetVideoOffset(draft, videoId)
      draft.videoDragPreview = null
      clampPlayheadToVideos(draft, 0)
    })
  },

  /** Shifts every clip by the same amount, keeping their relative placement. */
  shiftVideoOffsets: (deltaSeconds, { compensatePlayhead = false } = {}) => {
    set((draft) => {
      shiftVideoOffsets(draft, deltaSeconds)
      draft.videoDragPreview = null
      if (compensatePlayhead) clampPlayheadToVideos(draft, deltaSeconds)
    })
  },

  /** Sets or clears the transient shift of the dragged clip, or of every clip when `videoId` is null. */
  setVideoDragPreview: (dragPreview) => {
    if (dragPreview !== null) {
      if (dragPreview.videoId !== null && !get().importedVideos.some((video) => video.id === dragPreview.videoId)) {
        throw new Error(`Unknown imported video: ${dragPreview.videoId}`)
      }
      if (!Number.isFinite(dragPreview.deltaSeconds)) throw new Error('Video drag shift must be a finite number')
    }
    set({ videoDragPreview: dragPreview })
  },

  setVideoSyncTimezoneMode: (mode) => {
    set((draft) => setVideoSyncTimezoneMode(draft, mode))
  },

  /** Uses the `YYYYMMDD_HHMMSS` filename timestamp of every clip that has one. */
  setVideoCreationTimesFromFilenames: () => {
    set((draft) => applyFilenameCreationTimes(draft))
  },

  resetVideoCreationTimes: () => {
    set((draft) => resetCreationTimes(draft))
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
