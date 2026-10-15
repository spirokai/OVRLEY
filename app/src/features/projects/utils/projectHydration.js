import { clamp } from '@/lib/utils'
import { clampToView } from '@/features/player/utils/timelineViewport'
import { getTimelineMinimum } from '@/features/player/utils/playerTiming'
import { DEFAULT_RENDER_SETTINGS } from '@/store/slices/createRenderSettingsSlice'
import { activateParsedActivity } from '@/lib/activity/import-activity'

function assertLandmarksFitVideo(landmarks, videoDurationSeconds) {
  if (landmarks.length === 0) return
  if (videoDurationSeconds === null) {
    throw new Error('Manual video sync landmarks require an imported video')
  }
  const invalidLandmark = landmarks.find((landmark) => landmark.videoSecond > videoDurationSeconds)
  if (invalidLandmark) {
    throw new Error('Manual video sync landmark videoSecond must be within the imported video duration')
  }
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
    draft.renderSettings = {
      ...DEFAULT_RENDER_SETTINGS,
      range: { ...DEFAULT_RENDER_SETTINGS.range },
    }
    draft.errorMessage = null
    draft.batchVideoFolder = null
    draft.batchOutputFolder = null
    draft.batchQueue = []
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
  // Project format version 3 stores at most one video.
  const video = state.importedVideos[0] ?? null
  assertLandmarksFitVideo(project.sync.manual.landmarks, video?.durationSeconds ?? null)
  state.hydrateVideoSyncState({ ...project.sync.manual, videoId: video?.id ?? null })
  const videoBounds =
    video === null ? null : { startSecond: project.sync.videoOffsetSeconds, endSecond: project.sync.videoOffsetSeconds + video.durationSeconds }
  const timelineMinimum = getTimelineMinimum({ videoBounds })
  const activityDuration = state.activitySummary?.durationSeconds ?? 0
  const videoEnd = videoBounds?.endSecond ?? 0
  const timelineEnd = Math.max(activityDuration, videoEnd, timelineMinimum + 0.001)
  const timelineViewport = clampToView(project.timeline.viewStart, project.timeline.viewEnd, timelineEnd, timelineMinimum)
  const range = { ...project.render.range }
  const { batchVideoFolder, batchOutputFolder, ...renderSettings } = project.render

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
    if (video !== null) {
      draft.videoEdits[video.id].offsetSeconds = project.sync.videoOffsetSeconds
      draft.importedVideos[0].syncWarning = null
    }
    draft.videoSyncTimezoneMode = project.sync.videoTimezoneMode
    draft.videoDragPreview = null
    draft.renderSettings = { ...renderSettings, range }
    draft.batchVideoFolder = batchVideoFolder
    draft.batchOutputFolder = batchOutputFolder
    draft.batchQueue = []
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
 * @param {{activity: object|null, video: object|null}} prepared Prepared activity and registered video entity.
 */
export function applyPreparedProjectState(store, project, prepared) {
  store.setState((draft) => {
    draft.activitySource = null
    draft.parsedActivity = null
    draft.activitySummary = null
  })

  const state = store.getState()
  state.clearImportedVideos()
  if (prepared.video) state.setImportedVideos([prepared.video])
  if (prepared.activity) activateParsedActivity(prepared.activity, store.getState())

  applyProjectOwnedState(store, project)
}
