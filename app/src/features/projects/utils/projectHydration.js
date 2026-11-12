import { clamp } from '@/lib/utils'
import { clampToView } from '@/features/player/utils/timelineViewport'
import { getTimelineMinimum } from '@/features/player/utils/playerTiming'
import { DEFAULT_RENDER_SETTINGS } from '@/store/slices/createRenderSettingsSlice'
import { activateParsedActivity } from '@/lib/activity/import-activity'
import { MIN_ACTIVITY_TRIM_SECONDS, trimActivityEdge } from '@/lib/activity/activity-trim'
import { DEFAULT_TRANSITION_SETTINGS } from '@/lib/video-program/transition-catalog'
import { getVideosTimelineBounds, joinVideoEdits } from '@/lib/video-program/video-collection'
import { fitCutsToFrameCount } from '@/lib/video-program/video-segments'

// Laps belong to the external activity file; one loaded as absent takes its lap selection with it.
function resolveDisabledLaps(disabledLaps, parsedActivity) {
  if (parsedActivity === null) return []
  if (disabledLaps.length > 0 && disabledLaps.at(-1) >= parsedActivity.lap_start_elapsed_seconds.length) {
    throw new Error('Project laps.disabled must identify laps of the loaded activity')
  }
  return disabledLaps
}

/**
 * Restores the saved edits of every loaded video, missing ones included. The native
 * boundary validated their shape against the saved metadata; a found file shorter
 * than the saved video keeps only the cuts inside it. An anchor to a video removed
 * at load (an older project's missing video) is dropped; the clip keeps its offset.
 * @param {Array<object>} videos Loaded video entities with their saved IDs.
 * @param {Array<object>} projectVideos Saved project videos.
 * @returns {object} Video edits keyed by video ID.
 */
export function createProjectVideoEdits(videos, projectVideos) {
  const loadedIds = new Set(videos.map((video) => video.id))
  const projectVideosById = new Map(projectVideos.map((projectVideo) => [projectVideo.id, projectVideo]))
  return Object.fromEntries(
    videos.map((video) => {
      const projectVideo = projectVideosById.get(video.id)
      if (!projectVideo) throw new Error(`Imported video is not part of the project: ${video.id}`)
      return [
        video.id,
        {
          offsetSeconds: projectVideo.offsetSeconds,
          offsetSource: projectVideo.offsetSource,
          ...fitCutsToFrameCount(projectVideo, video.frameCount),
          placementAnchor:
            projectVideo.placementAnchor !== null && loadedIds.has(projectVideo.placementAnchor.videoId) ? { ...projectVideo.placementAnchor } : null,
          timezoneMode: projectVideo.timezoneMode,
        },
      ]
    }),
  )
}

/**
 * Restores the manual sync workspace. When its video was removed at load (an older
 * project's missing video), its landmarks go with it and the first loaded video is
 * selected. A found file shorter than the saved video keeps the landmarks inside it.
 * @param {Array<object>} videos Loaded video entities with their saved IDs.
 * @param {object} manual Saved manual sync workspace.
 * @returns {object} Manual sync workspace.
 */
export function createProjectManualVideoSync(videos, manual) {
  const video = videos.find((candidate) => candidate.id === manual.videoId)
  if (!video) return { ...manual, videoId: videos[0]?.id ?? null, landmarks: [] }
  return { ...manual, landmarks: manual.landmarks.filter((landmark) => landmark.videoSecond <= video.durationSeconds) }
}

/**
 * Replaces project-owned session state for a new project.
 * @param {object} store Zustand application store.
 * @param {object} options Loaded template baseline, if present.
 * @returns {void}
 */
export function applyNewProjectState(store, { templateSource, templateState }) {
  const state = store.getState()
  state.clearActivityFile()
  if (templateSource) {
    state.hydrateTemplateState(templateState, { source: templateSource })
  } else {
    state.createNewTemplate()
  }
  store.setState((draft) => {
    draft.activitySource = null
    draft.parsedActivity = null
    draft.activitySummary = null
    draft.fallbackDurationSeconds = 73
    draft.selectedSecond = 0
    draft.timelineViewport = { viewStart: 0, viewEnd: 73 }
    draft.previewPlaybackState = 'paused'
    draft.previewPlaybackSource = 'timeline'
    draft.videoDragPreview = null
    draft.transitionDefaults = { ...DEFAULT_TRANSITION_SETTINGS }
    draft.renderSettings = { ...DEFAULT_RENDER_SETTINGS }
    draft.errorMessage = null
    draft.batchOutputFolder = null
    draft.batchSnapshot = null
  })
  state.resetVideoSyncState()
}

/**
 * A saved trim of the activity loaded with the project, kept inside it and at least
 * the minimum length apart; a trim of an activity that is gone, or that covers it whole, is dropped.
 * @param {{startSecond: number, endSecond: number}|null} trim Saved trim.
 * @param {number} activityDuration Duration of the loaded activity, 0 without one.
 * @returns {{startSecond: number, endSecond: number}|null} Trim to restore.
 */
function resolveActivityTrim(trim, activityDuration) {
  if (trim === null || activityDuration <= MIN_ACTIVITY_TRIM_SECONDS) return null
  return trimActivityEdge(trimActivityEdge(null, activityDuration, 'end', trim.endSecond), activityDuration, 'start', trim.startSecond)
}

/**
 * Restores the project-owned editor widget document and settings after the
 * activity and the video collection have been committed.
 *
 * @param {object} store Zustand application store.
 * @param {object} project Validated project payload.
 * @param {object} manualVideoSync Manual sync workspace checked against the loaded videos.
 */
export function applyProjectOwnedState(store, project, manualVideoSync) {
  const state = store.getState()
  state.hydrateVideoSyncState(manualVideoSync)
  const disabledLaps = resolveDisabledLaps(project.laps.disabled, state.parsedActivity)
  const videoBounds = getVideosTimelineBounds(joinVideoEdits(state.importedVideos, state.videoEdits))
  const timelineMinimum = getTimelineMinimum({ videoBounds })
  const activityDuration = state.activitySummary?.durationSeconds ?? 0
  const videoEnd = videoBounds?.endSecond ?? 0
  const timelineEnd = Math.max(activityDuration, videoEnd, timelineMinimum + 0.001)
  const timelineViewport = clampToView(project.timeline.viewStart, project.timeline.viewEnd, timelineEnd, timelineMinimum)
  const { batchOutputFolder, ...renderSettings } = project.render
  const activityTrim = resolveActivityTrim(project.activityTrim, activityDuration)

  state.setConfig(project.editor.config)
  store.setState((draft) => {
    draft.globalDefaults = project.editor.globalDefaults
    draft.loadedTemplateSource = null
    draft.lastSavedTemplateState = null
    draft.disabledLaps = disabledLaps
    draft.transitionDefaults = { ...project.transitionDefaults }
    draft.videoDragPreview = null
    draft.cutRemovalRequest = null
    draft.renderSettings = renderSettings
    draft.activityTrim = activityTrim
    draft.batchOutputFolder = batchOutputFolder
    draft.batchSnapshot = null
    draft.selectedSecond = clamp(project.timeline.playheadSecond, timelineMinimum, timelineEnd)
    draft.timelineViewport = timelineViewport
    draft.skipNextTimelineViewportReset = true
    draft.previewPlaybackState = 'paused'
    draft.previewPlaybackSource = 'timeline'
  })
}

/**
 * Commits staged media through one synchronous project transition. The activity
 * is activated before the videos join, so detecting video times on activation
 * never moves the saved placements.
 * @param {object} store Zustand application store.
 * @param {object} project Validated project payload.
 * @param {{activity: object|null, collection: {importedVideos: Array<object>, videoEdits: object}, manualVideoSync: object}} prepared
 *   Prepared activity, registered video collection with its validated edits, and the manual sync workspace.
 */
export function applyPreparedProjectState(store, project, { activity, collection, manualVideoSync }) {
  store.setState((draft) => {
    draft.activitySource = null
    draft.parsedActivity = null
    draft.activitySummary = null
  })

  const state = store.getState()
  state.clearImportedVideos()
  if (activity) activateParsedActivity(activity, store.getState())
  store.getState().commitVideoCollection(collection)

  applyProjectOwnedState(store, project, manualVideoSync)
}
