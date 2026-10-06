import { captureVideoSyncTimezoneMode, createBatchCalibration } from '@/lib/video-sync'
import { buildPreviewFrameWindow } from '@/lib/preview-timing'
import { normalizeUpdateRateForFps } from '@/lib/update-rate'
import { validateRenderSettings } from '@/store/slices/createRenderSettingsSlice'
import { isQsvFullCodec } from './codecUtils'
import { createBatchRenderTemplate, createRenderEffectiveConfig } from './renderConfig'

/** @param {object} state Application state. @returns {boolean} Whether native rendering or submission owns the renderer. */
export function isRendererBusy(state) {
  return state.renderingVideo || (state.batchSnapshot?.rendererBusy ?? false) || state.renderSubmissionTarget !== null
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
  const { renderSettings, importedVideoPath } = state
  const fps = Math.trunc(renderSettings.fps)
  return {
    ...renderSettings,
    fps,
    widgetUpdateRate: normalizeUpdateRateForFps(fps, renderSettings.widgetUpdateRate),
    exportMode: renderSettings.renderTarget === 'batch' || importedVideoPath !== null ? renderSettings.exportMode : 'transparent',
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
  const hasExternalActivity = editorSnapshot.parsedActivitySource === 'activity-file'
  const activitySummary = hasExternalActivity ? editorSnapshot.activitySummary : null
  if (hasExternalActivity && activitySummary === null) throw new Error('External activity summary is required for batch synchronization')
  return { activitySummary, timezoneMode: captureVideoSyncTimezoneMode(editorSnapshot.videoSyncTimezoneMode) }
}

/** @param {object} options Captured editor/settings and inspected batch sources. @returns {Readonly<object>} Native BatchRenderRequest. */
export function createBatchRenderRequest({ editorSnapshot, settings, inspectionId, outputDirectory, jobs, calibrationSource, sync }) {
  const referenceVideo =
    sync.activitySummary !== null && editorSnapshot.importedVideoPath !== null
      ? {
          path: editorSnapshot.importedVideoPath,
          creationTime: editorSnapshot.importedVideoCreationTime,
          timeSource: editorSnapshot.importedVideoTimeSource,
          committedOffsetSeconds: editorSnapshot.videoSyncOffsetSeconds,
        }
      : null
  const calibration = createBatchCalibration({ activitySummary: sync.activitySummary, referenceVideo, timezoneMode: sync.timezoneMode })
  const activity =
    calibration.mode === 'embeddedActivity'
      ? { mode: 'embeddedActivity' }
      : {
          mode: 'externalActivity',
          activity: editorSnapshot.parsedActivity,
          timezoneMode: calibration.timezoneMode,
          reference:
            calibration.reference === null
              ? null
              : {
                  sourceId: calibrationSource.sourceId,
                  creationTime: calibration.reference.creationTime,
                  timeSource: calibration.reference.timeSource,
                  committedOffsetSeconds: calibration.reference.committedOffsetSeconds,
                  automaticOffsetSeconds: calibration.reference.automaticOffsetSeconds,
                },
          automaticOffsets: Object.fromEntries(
            jobs.map(({ source, timing }) => [
              source.sourceId,
              source.sourceId === calibrationSource?.sourceId ? calibration.reference.automaticOffsetSeconds : timing.automaticOffsetSeconds,
            ]),
          ),
        }
  return freezeRequest(
    structuredClone({
      inspectionId,
      template: createBatchRenderTemplate(editorSnapshot.config, editorSnapshot.globalDefaults),
      encoding: captureRenderEncoding(settings, editorSnapshot.availableCodecs),
      activity,
      outputDirectory,
      jobs: jobs.map(({ id, source, skipOverlay }) => ({ id, sourceId: source.sourceId, skipOverlay })),
    }),
  )
}

function createCurrentRenderConfig(state, settings) {
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
    range: settings.range,
    importedVideoPath: state.importedVideoPath,
    importedVideoDuration: state.importedVideoDuration,
    importedVideoFps: state.importedVideoFps,
    importedVideoFpsNum: state.importedVideoFpsNum,
    importedVideoFpsDen: state.importedVideoFpsDen,
    importedVideoResolution: state.importedVideoResolution,
    videoSyncOffsetSeconds: state.videoSyncOffsetSeconds,
    timelineStart: state.startSecond,
    timelineEnd: state.endSecond,
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
          parsedActivity: editorSnapshot.parsedActivity,
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
  if (state.parsedActivity === null) throw new Error('No parsed activity available')
  const hasVideo = state.importedVideoPath !== null
  const config = createCurrentRenderConfig(state, {
    ...state.renderSettings,
    exportMode: hasVideo ? 'composite' : 'transparent',
    codec: hasVideo ? 'libx264' : 'prores_ks',
  })
  const { start, end, fps } = config.scene
  const window = buildPreviewFrameWindow({ activityDuration: end - start, previewSecond: state.selectedSecond - start, sceneFps: fps })
  config.scene = { ...config.scene, start: start + window.start, end: start + window.end }
  return { config, parsedActivity: state.parsedActivity, second: state.selectedSecond }
}
