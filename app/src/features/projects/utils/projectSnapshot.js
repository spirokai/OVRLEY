import { createDurableEditorState } from '@/lib/widget/editor-state'
import { createPathLocator } from './projectPaths'

export const PROJECT_FORMAT = 'ovrley-project'
export const PROJECT_VERSION = 4
export const LAST_PROJECT_DIRECTORY_KEY = 'last-project-dir'

function cloneTransition(transition) {
  return transition === null ? null : { type: transition.type, durationSeconds: transition.durationSeconds }
}

/**
 * Projects the video collection in import order: identity, source locator, and user edits.
 * A `null` transition follows the project transition defaults.
 */
function projectVideos(state, projectPath) {
  return [...state.importedVideos]
    .sort((left, right) => left.importOrder - right.importOrder)
    .map((video) => {
      const edits = state.videoEdits[video.id]
      return {
        id: video.id,
        path: createPathLocator(video.path, projectPath),
        offsetSeconds: edits.offsetSeconds,
        deletedParts: edits.deletedParts.map((part) => ({
          id: part.id,
          fromSecond: part.fromSecond,
          toSecond: part.toSecond,
          transition: cloneTransition(part.transition),
        })),
        transitionIn: cloneTransition(edits.transitionIn),
        placementAnchor: edits.placementAnchor === null ? null : { ...edits.placementAnchor },
      }
    })
}

/**
 * Builds the project content that participates in dirty-state comparison.
 * @param {object} state Complete application state.
 * @param {string} projectPath Absolute destination path.
 * @returns {object} Persisted project content excluding timeline state.
 */
export function createProjectContentSnapshot(state, projectPath) {
  const source = (path) => (path ? { path: createPathLocator(path, projectPath) } : null)
  const { videoId, landmarks, detectedLocationSecond, speedThresholdKmh, turnThresholdDegrees } = state.manualVideoSync
  const editor = createDurableEditorState({ config: state.config, globalDefaults: state.globalDefaults })
  editor.config.scene.fps = state.renderSettings.fps
  editor.config.scene.updateRate = state.renderSettings.widgetUpdateRate

  return {
    editor,
    sources: {
      activity: source(state.activitySource?.path),
      videos: projectVideos(state, projectPath),
    },
    sync: {
      videoTimezoneMode: state.videoSyncTimezoneMode,
      manual: { videoId, landmarks, detectedLocationSecond, speedThresholdKmh, turnThresholdDegrees },
    },
    transitionDefaults: { ...state.transitionDefaults },
    render: {
      renderTarget: state.renderSettings.renderTarget,
      batchOutputFolder: state.batchOutputFolder,
      fps: state.renderSettings.fps,
      widgetUpdateRate: state.renderSettings.widgetUpdateRate,
      exportMode: state.renderSettings.renderTarget === 'batch' || state.importedVideos.length > 0 ? state.renderSettings.exportMode : 'transparent',
      codec: state.renderSettings.codec,
      qualityType: state.renderSettings.qualityType,
      qualityValue: state.renderSettings.qualityValue,
      range: { ...state.renderSettings.range },
    },
  }
}

/**
 * Explicitly projects only project-owned durable state.
 * @param {object} state Complete application state.
 * @param {string} projectPath Absolute destination project path.
 * @returns {object} Canonical version 4 payload.
 */
export function createProjectSnapshot(state, projectPath) {
  return {
    format: PROJECT_FORMAT,
    version: PROJECT_VERSION,
    savedAt: new Date().toISOString(),
    ...createProjectContentSnapshot(state, projectPath),
    rasterAssets: {},
    timeline: {
      playheadSecond: state.selectedSecond,
      viewStart: state.timelineViewport.viewStart,
      viewEnd: state.timelineViewport.viewEnd,
    },
  }
}

/**
 * Builds the saved baseline used by dirty-state tracking.
 * @param {object} project Canonical project payload.
 * @returns {{ content: object, timeline: object }} Saved content and timeline baselines.
 */
export function createProjectDirtyState(project) {
  return {
    content: {
      editor: project.editor,
      sources: project.sources,
      sync: project.sync,
      transitionDefaults: project.transitionDefaults,
      render: project.render,
    },
    timeline: project.timeline,
  }
}

/** @param {object} project Canonical payload. */
export function stringifyProject(project) {
  return JSON.stringify(project, null, 2)
}
