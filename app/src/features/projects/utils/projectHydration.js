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
 * their shape; bounds that need the probed duration are checked here. An anchor
 * to a video skipped as a missing source is dropped; the clip keeps its offset.
 */
function createProjectVideoEdits(videos, projectVideos) {
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
 */
function createProjectManualVideoSync(videos, manual) {
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
 * activity and video loaders have populated their own Zustand domains.
 *
 * @param {object} store Zustand application store.
 * @param {object} project Validated project payload.
 */
export function applyProjectOwnedState(store, project) {
  const state = store.getState()
  const videoEdits = createProjectVideoEdits(state.importedVideos, project.sources.videos)
  state.hydrateVideoSyncState(createProjectManualVideoSync(state.importedVideos, project.sync.manual))
  const videoBounds = getVideosTimelineBounds(joinVideoEdits(state.importedVideos, videoEdits))
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
    draft.videoEdits = videoEdits
    for (const video of draft.importedVideos) video.syncWarning = null
    draft.videoSyncTimezoneMode = project.sync.videoTimezoneMode
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
 * Commits staged media through one synchronous project transition.
 * @param {object} store Zustand application store.
 * @param {object} project Validated project payload.
 * @param {{activity: object|null, videos: Array<object>}} prepared Prepared activity and registered video entities.
 */
export function applyPreparedProjectState(store, project, prepared) {
  store.setState((draft) => {
    draft.activitySource = null
    draft.parsedActivity = null
    draft.activitySummary = null
  })

  const state = store.getState()
  state.clearImportedVideos()
  if (prepared.videos.length > 0) state.setImportedVideos(prepared.videos)
  if (prepared.activity) activateParsedActivity(prepared.activity, store.getState())

  applyProjectOwnedState(store, project)
}
