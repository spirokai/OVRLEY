import { detectCodecs } from '@/api/backend'
import { getTimelineMinimum, getTotalPlaybackDuration } from '@/features/player/utils/playerTiming'
import { videoIdFromLaneId, videoLaneId } from '@/features/player/utils/timelineGeometry'
import { createCachedPromise } from '@/lib/cached-promise'
import { clamp } from '@/lib/utils'
import {
  applyFilenameCreationTimes,
  cutVideo,
  findVideo,
  moveCut,
  placeAutomaticVideos,
  removeCut,
  removeVideo,
  resetCreationTimes,
  resetVideoOffset,
  segmentBeforeCut,
  setSegmentExcluded,
  setSegmentTransition,
  setVideoCreationTimeSource,
  setVideoOffset,
  setVideosTimezoneMode,
  setVideoTimezoneMode,
  shiftVideoOffsets,
  syncVideos,
  trimVideo,
} from '@/lib/video-program/collection-edits'
import { requireTelemetry } from '@/lib/video-program/imported-video'
import { DEFAULT_TRANSITION_SETTINGS, requireTransitionDefaults } from '@/lib/video-program/transition-catalog'
import { getVideosTimelineBounds, joinVideoEdits, sortVideosByTimeline } from '@/lib/video-program/video-collection'
import { fitCutsToFrameCount } from '@/lib/video-program/video-segments'
import { applyCoursePlotDimensions } from '../store-utils'

let fetchCodecsOnce = null

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
  cutRemovalRequest: null, // { videoId, cutId } while the user chooses the state of the merged segment
  transitionDefaults: { ...DEFAULT_TRANSITION_SETTINGS },
  availableCodecs: null,
  selectedClip: null, // { laneId, segmentKey } of the clip selected on the timeline or in the video panel; the video drawer shows its video

  /**
   * Replaces the collection with video entities and their edits (a reviewed import or
   * a loaded project). Every video but a missing one is registered for preview. A
   * manual-sync selection of a video no longer in the collection moves to the first
   * clip; the playhead stays on the timeline.
   * @param {{importedVideos: Array<object>, videoEdits: object}} collection Collection to commit.
   */
  commitVideoCollection: ({ importedVideos, videoEdits }) => {
    if (importedVideos.some((video) => !video.missing && video.preview === null)) throw new Error('Committed videos must be registered for preview')
    set((draft) => {
      draft.importedVideos = importedVideos
      draft.videoEdits = videoEdits
      draft.importedBackgroundImagePath = null
      draft.videoDragPreview = null
      draft.cutRemovalRequest = null
      const selectedVideoId = draft.selectedClip === null ? null : videoIdFromLaneId(draft.selectedClip.laneId)
      if (selectedVideoId !== null && !importedVideos.some((video) => video.id === selectedVideoId)) draft.selectedClip = null
      clampPlayheadToVideos(draft, 0)
    })
    const { manualVideoSync } = get()
    if (!importedVideos.some((video) => video.id === manualVideoSync.videoId)) {
      const [first] = sortVideosByTimeline(joinVideoEdits(importedVideos, videoEdits))
      get().selectVideoSyncVideo(first?.id ?? null)
    }
  },

  /**
   * Replaces a missing video with the entity probed from the file the user located,
   * keeping the video's edits. Cuts and manual-sync landmarks past the end of a
   * shorter file are dropped.
   * @param {object} video Registered entity with the missing video's ID.
   */
  relinkVideo: (video) => {
    if (video.missing || video.preview === null) throw new Error('A relinked video must be registered for preview')
    set((draft) => {
      const index = draft.importedVideos.findIndex((candidate) => candidate.id === video.id)
      if (index === -1 || !draft.importedVideos[index].missing) throw new Error(`Video is not missing: ${video.id}`)
      draft.importedVideos[index] = video
      const edits = draft.videoEdits[video.id]
      Object.assign(edits, fitCutsToFrameCount(edits, video.frameCount))
      if (draft.manualVideoSync.videoId === video.id) {
        draft.manualVideoSync.landmarks = draft.manualVideoSync.landmarks.filter((landmark) => landmark.videoSecond <= video.durationSeconds)
      }
      const selection = draft.selectedClip
      if (selection?.laneId === videoLaneId(video.id) && selection.segmentKey !== null && edits.segments[selection.segmentKey] === undefined) {
        selection.segmentKey = null
      }
      draft.videoDragPreview = null
      clampPlayheadToVideos(draft, 0)
    })
  },

  /**
   * Selects a timeline clip, and on a video clip the segment under the pointer, or
   * nothing. The timeline, the video panel, and the video drawer share this selection.
   * @param {{laneId: string, segmentKey: string|null}|null} selection Lane of the clip and segment key (null for the whole clip).
   */
  selectClip: (selection) => {
    const videoId = selection === null ? null : videoIdFromLaneId(selection.laneId)
    if (videoId !== null) findVideo(get(), videoId)
    set({ selectedClip: selection })
  },

  setImportedBackgroundImage: (path) => {
    set({
      importedVideos: [],
      videoEdits: {},
      importedBackgroundImagePath: path || null,
      videoDragPreview: null,
      cutRemovalRequest: null,
      selectedClip: null,
    })
    get().selectVideoSyncVideo(null)
  },

  clearImportedVideos: () => {
    set({
      importedVideos: [],
      videoEdits: {},
      importedBackgroundImagePath: null,
      videoDragPreview: null,
      cutRemovalRequest: null,
      selectedClip: null,
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
      if (draft.cutRemovalRequest?.videoId === videoId) draft.cutRemovalRequest = null
      if (draft.selectedClip?.laneId === videoLaneId(videoId)) draft.selectedClip = null
      clampPlayheadToVideos(draft, 0)
    })
    if (get().manualVideoSync.videoId === videoId) {
      const [first] = sortVideosByTimeline(joinVideoEdits(get().importedVideos, get().videoEdits))
      get().selectVideoSyncVideo(first?.id ?? null)
    }
  },

  /** Stores the project transition defaults chosen by the user. */
  setTransitionDefaults: (defaults) => {
    set({ transitionDefaults: requireTransitionDefaults(defaults) })
  },

  /** Places every automatic clip by detected time (see `placeAutomaticVideos`). */
  computeVideoSync: () => {
    set((draft) => placeAutomaticVideos(draft))
  },

  /** Places every clip by its detected recording time again (see `syncVideos`). */
  syncAllVideos: () => {
    set((draft) => syncVideos(draft))
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

  /**
   * Splits a video at a frame (see `cutVideo`).
   * @returns {string} ID of the new cut.
   */
  cutVideo: (videoId, frame) => {
    let cutId
    set((draft) => {
      cutId = cutVideo(draft, videoId, frame)
    })
    return cutId
  },

  /** Trims the start or end of a video, excluding the outer part (see `trimVideo`). */
  trimVideo: (videoId, side, frame) => {
    set((draft) => {
      trimVideo(draft, videoId, side, frame)
    })
  },

  /** Moves a cut to another frame between its neighbours (see `moveCut`). */
  moveCut: (videoId, cutId, frame) => {
    set((draft) => moveCut(draft, videoId, cutId, frame))
  },

  /**
   * Removes a cut. Segments in the same export state merge at once; otherwise the
   * user chooses the state of the merged segment (`resolveCutRemoval`).
   */
  requestCutRemoval: (videoId, cutId) => {
    const edits = get().videoEdits[videoId]
    const earlier = edits.segments[segmentBeforeCut(edits, cutId)]
    if (earlier.excluded === edits.segments[cutId].excluded) {
      set((draft) => removeCut(draft, videoId, cutId, earlier.excluded))
      return
    }
    set({ cutRemovalRequest: { videoId, cutId } })
  },

  /** Removes the requested cut, giving the merged segment the chosen export state. */
  resolveCutRemoval: (excluded) => {
    const { videoId, cutId } = get().cutRemovalRequest
    set((draft) => {
      removeCut(draft, videoId, cutId, excluded)
      draft.cutRemovalRequest = null
    })
  },

  dismissCutRemoval: () => {
    set({ cutRemovalRequest: null })
  },

  /** Excludes a segment from export or includes it again. */
  setVideoSegmentExcluded: (videoId, segmentKey, excluded) => {
    set((draft) => setSegmentExcluded(draft, videoId, segmentKey, excluded))
  },

  /**
   * Overrides the transition of a program join, or with `null` returns it to the defaults.
   * @param {{videoId: string, segmentKey: string}} join Join from `buildProgram`.
   * @param {{type: string, durationSeconds: number}|null} transition Override.
   */
  setJoinTransition: ({ videoId, segmentKey }, transition) => {
    set((draft) => setSegmentTransition(draft, videoId, segmentKey, transition))
  },

  /** Sets or clears the transient shift of the dragged clip, or of every clip when `videoId` is null. */
  setVideoDragPreview: (dragPreview) => {
    if (dragPreview !== null) {
      if (dragPreview.videoId !== null) findVideo(get(), dragPreview.videoId)
      if (!Number.isFinite(dragPreview.deltaSeconds)) throw new Error('Video drag shift must be a finite number')
    }
    set({ videoDragPreview: dragPreview })
  },

  /** Reads every clip's ambiguous camera clock in local time or UTC (see `setVideosTimezoneMode`). */
  setVideoSyncTimezoneMode: (mode) => {
    set((draft) => setVideosTimezoneMode(draft, mode))
  },

  /** Reads one clip's ambiguous camera clock in local time or UTC (see `setVideoTimezoneMode`). */
  setVideoTimezoneMode: (videoId, mode) => {
    set((draft) => setVideoTimezoneMode(draft, videoId, mode))
  },

  /** Takes one clip's recording time from its detected time or its filename (see `setVideoCreationTimeSource`). */
  setVideoCreationTimeSource: (videoId, source) => {
    set((draft) => setVideoCreationTimeSource(draft, videoId, source))
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
