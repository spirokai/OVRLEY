import { getActivityDurationSeconds } from '@/lib/activity/activity-summary'
import { joinVideoEdits } from '@/lib/video-program/video-collection'
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

/**
 * Resolves the clip, activity, and activity window of the current-video render.
 * With an external activity the clip keeps its committed offset; without one the
 * clip's own telemetry drives the overlay in video-local time.
 * @param {object} state Editor snapshot.
 * @returns {{clip: object|null, activity: object|null, originSecond: number, timelineStart: number, timelineEnd: number}} Render inputs.
 */
export function resolveCurrentRenderInputs(state) {
  const clips = joinVideoEdits(state.importedVideos, state.videoEdits)
  // Rendering several videos arrives with the batch program; until then the collection holds at most one video.
  if (clips.length > 1) throw new Error('Exporting several videos is not supported yet')
  const clip = clips[0] ?? null
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
    exportMode: renderSettings.renderTarget === 'batch' || importedVideos.length > 0 ? renderSettings.exportMode : 'transparent',
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
 * Builds the native separate-file export of every reviewed video. With an external
 * activity each video renders at its committed offset; without one each video
 * uses its own embedded telemetry.
 * @param {object} options Captured editor/settings and reviewed, inspected jobs.
 * @returns {Readonly<object>} Native BatchRenderRequest.
 */
export function createBatchRenderRequest({ editorSnapshot, settings, inspectionId, outputDirectory, jobs }) {
  const activity =
    editorSnapshot.parsedActivity === null
      ? { mode: 'embeddedActivity' }
      : {
          mode: 'externalActivity',
          activity: editorSnapshot.parsedActivity,
          offsets: Object.fromEntries(jobs.map(({ source, offsetSeconds }) => [source.sourceId, offsetSeconds])),
        }
  return freezeRequest(
    structuredClone({
      inspectionId,
      template: createBatchRenderTemplate(editorSnapshot.config, editorSnapshot.globalDefaults),
      encoding: captureRenderEncoding(settings, editorSnapshot.availableCodecs),
      activity,
      outputDirectory,
      // Job IDs are the source paths, which are unique in the collection.
      jobs: jobs.map(({ path, source, skipOverlay }) => ({ id: path, sourceId: source.sourceId, skipOverlay })),
    }),
  )
}

function createCurrentRenderConfig(state, settings) {
  const { clip, originSecond, timelineStart, timelineEnd } = resolveCurrentRenderInputs(state)
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

/** @param {object} options Editor snapshot, canonical settings, optional batch review and overwrite choice. @returns {object} Captured execution payload and accepted settings. */
export function createRenderRequest({ editorSnapshot, settings, batchReview, overwrite = false }) {
  validateRenderSettings(settings)
  const { outputPath, renderTarget, fps, widgetUpdateRate, exportMode, codec, qualityType, qualityValue, range } = settings
  const acceptedSettings = { renderTarget, fps, widgetUpdateRate, exportMode, codec, qualityType, qualityValue, range: { ...range } }
  const target = settings.renderTarget
  const payload =
    target === 'batch'
      ? createBatchRenderRequest({ editorSnapshot, settings, ...batchReview })
      : {
          config: createCurrentRenderConfig(editorSnapshot, settings),
          parsedActivity: resolveCurrentRenderInputs(editorSnapshot).activity,
          outputPath,
          overwrite,
        }
  if (target === 'current') {
    if (payload.parsedActivity === null) throw new Error('No parsed activity available')
    if (!outputPath) throw new Error('Render output target is required')
    if (payload.config.scene.start >= payload.config.scene.end) throw new Error('Start time must be before end time')
  }
  return { target, payload, settings: acceptedSettings }
}

/** @param {object} state Captured editor snapshot. @returns {object} Native preview frame inputs. */
export function createPreviewRenderRequest(state) {
  const { activity, originSecond } = resolveCurrentRenderInputs(state)
  if (activity === null) throw new Error('No parsed activity available')
  const hasVideo = state.importedVideos.length > 0
  const config = createCurrentRenderConfig(state, {
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
