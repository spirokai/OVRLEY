import { getActivityDurationSeconds } from '@/lib/activity/activity-summary'
import { resolvePreviewTarget } from '@/lib/video-program/playback-target'
import { buildTimelineProgram } from '@/lib/video-program/timeline'
import { captureVideoSyncTimezoneMode } from '@/lib/video-sync'
import { buildPreviewFrameWindow } from '@/lib/preview-timing'
import { normalizeUpdateRateForFps } from '@/lib/update-rate'
import { validateRenderSettings } from '@/store/slices/createRenderSettingsSlice'
import { isQsvFullCodec } from './codecUtils'
import { createBatchRenderTemplate, createRenderEffectiveConfig } from './renderConfig'

/** @param {object} state Application state. @returns {boolean} Whether native rendering or submission owns the renderer. */
export function isRendererBusy(state) {
  return state.renderingVideo || (state.batchSnapshot?.rendererBusy ?? false) || state.renderSubmissionTarget !== null
}

/** @param {object} state Editor snapshot. @returns {'single'|'batch'} Native transport: videos render through the batch service. */
export function renderTransport(state) {
  return state.importedVideos.length > 0 ? 'batch' : 'single'
}

/**
 * Resolves the clip, activity, and activity window of a single native render:
 * the activity overlay without videos, or a preview frame of the clip the
 * preview shows at the playhead. With an external activity the clip keeps its
 * committed offset; without one the clip's own telemetry drives the overlay in
 * video-local time.
 * @param {object} state Editor snapshot.
 * @returns {{clip: object|null, activity: object|null, originSecond: number, timelineStart: number, timelineEnd: number}} Render inputs.
 */
export function resolveSingleRenderInputs(state) {
  const { clips, program } = buildTimelineProgram({
    videos: state.importedVideos,
    videoEdits: state.videoEdits,
    dragPreview: null,
    transitionDefaults: state.transitionDefaults,
  })
  const target = resolvePreviewTarget(program, state.selectedSecond)
  const clip = target === null ? null : clips.find((candidate) => candidate.id === target.videoId)
  if (state.parsedActivity !== null || clip === null) {
    return { clip, activity: state.parsedActivity, originSecond: 0, timelineStart: state.startSecond, timelineEnd: state.endSecond }
  }
  const activity = clip.telemetry.activity
  return {
    clip,
    activity,
    originSecond: clip.offsetSeconds,
    timelineStart: 0,
    timelineEnd: activity === null ? 0 : getActivityDurationSeconds(activity),
  }
}

function shiftRange(range, originSecond) {
  return { ...range, from: range.from - originSecond, to: range.to - originSecond }
}

function freezeRequest(value) {
  if (value !== null && typeof value === 'object') {
    for (const child of Object.values(value)) freezeRequest(child)
    Object.freeze(value)
  }
  return value
}

/** @param {object} state Editor snapshot. @returns {object} Editable settings using the store's canonical field names. */
export function createRenderSettingsDraft(state) {
  const { renderSettings, importedVideos } = state
  const fps = Math.trunc(renderSettings.fps)
  return {
    ...renderSettings,
    fps,
    widgetUpdateRate: normalizeUpdateRateForFps(fps, renderSettings.widgetUpdateRate),
    exportMode: renderSettings.outputMode === 'separate' || importedVideos.length > 0 ? renderSettings.exportMode : 'transparent',
    range: { ...renderSettings.range },
  }
}

/** @param {object} settings Canonical render settings. @param {object|null} availableCodecs Native encoder capabilities. @returns {object} Rust encoding contract. */
export function captureRenderEncoding(settings, availableCodecs) {
  const { exportMode, codec, fps, widgetUpdateRate, qualityType, qualityValue } = settings
  const qsvFullInitArgs = isQsvFullCodec(codec) ? (availableCodecs?.qsvFullInitArgs ?? null) : null
  return { exportMode, exportCodec: codec, fps, updateRate: widgetUpdateRate, qualityType, qualityValue, qsvFullInitArgs }
}

/** @param {object} editorSnapshot Editor synchronization inputs. @returns {object} Automatic synchronization context for inspection and submission. */
export function captureBatchSync(editorSnapshot) {
  const hasExternalActivity = editorSnapshot.parsedActivity !== null
  const activitySummary = hasExternalActivity ? editorSnapshot.activitySummary : null
  if (hasExternalActivity && activitySummary === null) throw new Error('External activity summary is required for batch synchronization')
  return { activitySummary, timezoneMode: captureVideoSyncTimezoneMode(editorSnapshot.videoSyncTimezoneMode) }
}

/**
 * Native destination of an export: one combined file at the output path, or one
 * file per video in the output folder named after its source.
 * @param {object} options Output mode, combined path, separate folder and overwrite choice.
 * @returns {{kind: 'file', path: string, overwrite: boolean}|{kind: 'sourceNamed', directory: string}|null}
 *   Native BatchDestination, or null while the path or folder is missing.
 */
export function createExportDestination({ outputMode, outputPath, outputDirectory, overwrite }) {
  if (outputMode === 'combined') return outputPath === '' ? null : { kind: 'file', path: outputPath, overwrite }
  return outputDirectory === null ? null : { kind: 'sourceNamed', directory: outputDirectory }
}

/**
 * Builds the native export of reviewed outputs. With an external activity each
 * source renders at its committed offset; without one each source uses its own
 * embedded telemetry.
 * @param {object} options Captured editor/settings, the inspection, reviewed outputs with their sources' offsets, and the destination.
 * @returns {Readonly<object>} Native BatchRenderRequest.
 */
export function createBatchRenderRequest({ editorSnapshot, settings, inspectionId, outputs, offsets, destination }) {
  const activity =
    editorSnapshot.parsedActivity === null
      ? { mode: 'embeddedActivity' }
      : { mode: 'externalActivity', activity: editorSnapshot.parsedActivity, offsets }
  return freezeRequest(
    structuredClone({
      inspectionId,
      template: createBatchRenderTemplate(editorSnapshot.config, editorSnapshot.globalDefaults),
      encoding: captureRenderEncoding(settings, editorSnapshot.availableCodecs),
      activity,
      destination,
      outputs,
    }),
  )
}

function createSingleRenderConfig(state, settings) {
  const { clip, originSecond, timelineStart, timelineEnd } = resolveSingleRenderInputs(state)
  return createRenderEffectiveConfig({
    config: state.config,
    globalDefaults: state.globalDefaults,
    availableCodecs: state.availableCodecs,
    fps: settings.fps,
    widgetUpdateRate: settings.widgetUpdateRate,
    codec: settings.codec,
    exportMode: settings.exportMode,
    qualityType: settings.qualityType,
    qualityValue: settings.qualityValue,
    range: shiftRange(settings.range, originSecond),
    video: clip,
    videoOffsetSeconds: clip === null ? 0 : clip.offsetSeconds - originSecond,
    timelineStart,
    timelineEnd,
  })
}

/**
 * Captures one submission. With videos the batch service renders the reviewed
 * outputs (combined or separate); without videos a single render produces the
 * activity overlay.
 * @param {object} options Editor snapshot, canonical settings, the batch review for videos, and the overwrite choice.
 * @returns {{target: 'single'|'batch', payload: object, settings: object}} Native transport, payload and accepted settings.
 */
export function createRenderRequest({ editorSnapshot, settings, batchReview, overwrite = false }) {
  validateRenderSettings(settings)
  const { outputPath, outputMode, fps, widgetUpdateRate, exportMode, codec, qualityType, qualityValue, range } = settings
  const acceptedSettings = { outputMode, fps, widgetUpdateRate, exportMode, codec, qualityType, qualityValue, range: { ...range } }
  const target = renderTransport(editorSnapshot)
  if (target === 'batch') {
    const destination = createExportDestination({ outputMode, outputPath, outputDirectory: editorSnapshot.batchOutputFolder, overwrite })
    if (destination === null) throw new Error('Render output target is required')
    const { inspectionId, outputs, offsets } = batchReview
    return {
      target,
      payload: createBatchRenderRequest({ editorSnapshot, settings, inspectionId, outputs, offsets, destination }),
      settings: acceptedSettings,
    }
  }
  const payload = {
    config: createSingleRenderConfig(editorSnapshot, settings),
    parsedActivity: resolveSingleRenderInputs(editorSnapshot).activity,
    outputPath,
    overwrite,
  }
  if (payload.parsedActivity === null) throw new Error('No parsed activity available')
  if (!outputPath) throw new Error('Render output target is required')
  if (payload.config.scene.start >= payload.config.scene.end) throw new Error('Start time must be before end time')
  return { target, payload, settings: acceptedSettings }
}

/** @param {object} state Captured editor snapshot. @returns {object} Native preview frame inputs. */
export function createPreviewRenderRequest(state) {
  const { activity, originSecond } = resolveSingleRenderInputs(state)
  if (activity === null) throw new Error('No parsed activity available')
  const hasVideo = state.importedVideos.length > 0
  const config = createSingleRenderConfig(state, {
    ...state.renderSettings,
    exportMode: hasVideo ? 'composite' : 'transparent',
    codec: hasVideo ? 'libx264' : 'prores_ks',
  })
  const { start, end, fps } = config.scene
  const activitySecond = state.selectedSecond - originSecond
  const window = buildPreviewFrameWindow({ activityDuration: end - start, previewSecond: activitySecond - start, sceneFps: fps })
  config.scene = { ...config.scene, start: start + window.start, end: start + window.end }
  return { config, parsedActivity: activity, second: activitySecond }
}
