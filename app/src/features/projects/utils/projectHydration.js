import { clamp } from '@/lib/utils'
import { clampToView } from '@/features/player/utils/timelineViewport'
import { getTimelineMinimum } from '@/features/player/utils/playerTiming'
import { DEFAULT_RENDER_SETTINGS } from '@/store/slices/createRenderSettingsSlice'
import { activateParsedActivity } from '@/lib/activity/import-activity'
import { DEFAULT_TRANSITION_SETTINGS } from '@/lib/video-program/transition-catalog'
import { getVideosTimelineBounds, joinVideoEdits } from '@/lib/video-program/video-collection'

function cloneTransition(transition) {
  return transition === null ? null : { type: transition.type, durationSeconds: transition.durationSeconds }
}

/**
 * Restores the saved edits of every loaded video. The native boundary validated
 * their shape; bounds that need the probed duration are checked here, before the
 * project replaces the session. An anchor to a video skipped as a missing source
 * is dropped; the clip keeps its offset.
 * @param {Array<object>} videos Probed video entities with their saved IDs.
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
      if (projectVideo.deletedParts.some((part) => part.toSecond > video.durationSeconds)) {
        throw new Error('Deleted video parts must be within the imported video duration')
      }
      return [
        video.id,
        {
          offsetSeconds: projectVideo.offsetSeconds,
          offsetSource: projectVideo.offsetSource,
          deletedParts: projectVideo.deletedParts.map((part) => ({ ...part, transition: cloneTransition(part.transition) })),
          transitionIn: cloneTransition(projectVideo.transitionIn),
          placementAnchor:
            projectVideo.placementAnchor !== null && loadedIds.has(projectVideo.placementAnchor.videoId) ? { ...projectVideo.placementAnchor } : null,
        },
      ]
    }),
  )
}

/**
 * Restores the manual sync workspace. When its video was skipped as a missing
 * source, its landmarks go with it and the first loaded video is selected.
 * Landmarks are checked against the probed video before the project replaces the session.
 * @param {Array<object>} videos Probed video entities with their saved IDs.
 * @param {object} manual Saved manual sync workspace.
 * @returns {object} Manual sync workspace.
 */
export function createProjectManualVideoSync(videos, manual) {
  const video = videos.find((candidate) => candidate.id === manual.videoId)
  if (!video) return { ...manual, videoId: videos[0]?.id ?? null, landmarks: [] }
  if (manual.landmarks.some((landmark) => landmark.videoSecond > video.durationSeconds)) {
    throw new Error('Manual video sync landmark videoSecond must be within the imported video duration')
  }
  return manual
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
    draft.startSecond = 0
    draft.endSecond = 73
    draft.selectedSecond = 0
    draft.timelineViewport = { viewStart: 0, viewEnd: 73 }
    draft.previewPlaybackState = 'paused'
    draft.previewPlaybackSource = 'timeline'
    draft.videoDragPreview = null
    draft.videoSyncTimezoneMode = null
    draft.transitionDefaults = { ...DEFAULT_TRANSITION_SETTINGS }
    draft.renderSettings = {
      ...DEFAULT_RENDER_SETTINGS,
      range: { ...DEFAULT_RENDER_SETTINGS.range },
    }
    draft.errorMessage = null
    draft.batchOutputFolder = null
    draft.batchSkipOverlay = {}
    draft.batchSnapshot = null
  })
  state.resetVideoSyncState()
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
  const videoBounds = getVideosTimelineBounds(joinVideoEdits(state.importedVideos, state.videoEdits))
  const timelineMinimum = getTimelineMinimum({ videoBounds })
  const activityDuration = state.activitySummary?.durationSeconds ?? 0
  const videoEnd = videoBounds?.endSecond ?? 0
  const timelineEnd = Math.max(activityDuration, videoEnd, timelineMinimum + 0.001)
  const timelineViewport = clampToView(project.timeline.viewStart, project.timeline.viewEnd, timelineEnd, timelineMinimum)
  const range = { ...project.render.range }
  const { batchOutputFolder, ...renderSettings } = project.render

  if (range.type === 'custom') {
    range.from = clamp(range.from, timelineMinimum, timelineEnd)
    range.to = clamp(range.to, timelineMinimum, timelineEnd)
    if (range.from >= range.to) {
      range.from = timelineMinimum
      range.to = timelineEnd
    }
  }

  state.setConfig(project.editor.config)
  store.setState((draft) => {
    draft.globalDefaults = project.editor.globalDefaults
    draft.loadedTemplateSource = null
    draft.lastSavedTemplateState = null
    draft.transitionDefaults = { ...project.transitionDefaults }
    draft.videoDragPreview = null
    draft.deletedPartMark = null
    draft.renderSettings = { ...renderSettings, range }
    draft.batchOutputFolder = batchOutputFolder
    draft.batchSkipOverlay = {}
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
 * @param {{activity: object|null, collection: {importedVideos: Array<object>, videoEdits: object, videoSyncTimezoneMode: 'utc'|'local'|null}, manualVideoSync: object}} prepared
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
