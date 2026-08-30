import { createDurableEditorState } from '@/lib/widget/editor-state'
import { createPathLocator } from './projectPaths'

export const PROJECT_FORMAT = 'ovrley-project'
export const PROJECT_VERSION = 1
export const LAST_PROJECT_DIRECTORY_KEY = 'last-project-dir'

/**
 * Explicitly projects only project-owned durable state.
 * @param {object} state Complete application state.
 * @param {string} projectPath Absolute destination project path.
 * @returns {object} Canonical version 1 payload.
 */
export function createProjectSnapshot(state, projectPath) {
  const source = (path) => (path ? { path: createPathLocator(path, projectPath) } : null)
  const editor = createDurableEditorState({ config: state.config, globalDefaults: state.globalDefaults })
  editor.config.scene.fps = state.renderSettings.fps
  editor.config.scene.updateRate = state.renderSettings.widgetUpdateRate
  return {
    format: PROJECT_FORMAT,
    version: PROJECT_VERSION,
    savedAt: new Date().toISOString(),
    editor,
    sources: {
      activity: source(state.activitySource?.path),
      video: source(state.importedVideoPath),
    },
    sync: {
      videoOffsetSeconds: state.videoSyncOffsetSeconds,
      videoTimezoneMode: state.videoSyncTimezoneMode,
    },
    render: {
      fps: state.renderSettings.fps,
      widgetUpdateRate: state.renderSettings.widgetUpdateRate,
      exportMode: state.importedVideoPath ? state.renderSettings.exportMode : 'transparent',
      codec: state.renderSettings.codec,
      bitrateMbps: state.renderSettings.bitrateMbps,
      range: { ...state.renderSettings.range },
    },
    timeline: {
      playheadSecond: state.selectedSecond,
      viewStart: state.timelineViewport.viewStart,
      viewEnd: state.timelineViewport.viewEnd,
    },
  }
}

/** @param {object} project Canonical project payload. */
export function createProjectDirtyProjection(project) {
  return {
    editor: project.editor,
    sources: project.sources,
    sync: project.sync,
    render: project.render,
    timeline: project.timeline,
  }
}

/** @param {object} project Canonical payload. */
export function stringifyProject(project) {
  return JSON.stringify(project, null, 2)
}
