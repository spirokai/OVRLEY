import { createBatchCalibration, resolveBatchVideoTiming } from '@/lib/video-sync'
import { isQsvFullCodec } from './render-execution'
import { createBatchRenderTemplate } from './renderConfig'

function freezeRequest(value) {
  if (value !== null && typeof value === 'object') {
    for (const child of Object.values(value)) freezeRequest(child)
    Object.freeze(value)
  }
  return value
}

/** @param {object} settings Dialog settings. @param {object|null} availableCodecs Detected hardware. @returns {object} Native encoder inputs. */
export function captureBatchEncoding(settings, availableCodecs) {
  const { exportMode, exportCodec, fps, updateRate, qualityType, qualityValue } = settings
  const qsvFullInitArgs = isQsvFullCodec(exportCodec) ? (availableCodecs?.qsvFullInitArgs ?? null) : null
  return { exportMode, exportCodec, fps, updateRate, qualityType, qualityValue, qsvFullInitArgs }
}

/** @param {object} editorSnapshot Editor sync inputs. @returns {object} Shared synchronization context for review and submission. */
export function captureBatchSync(editorSnapshot) {
  const hasExternalActivity = editorSnapshot.parsedActivitySource === 'activity-file'
  const activitySummary = hasExternalActivity ? editorSnapshot.activitySummary : null
  if (hasExternalActivity && activitySummary === null) throw new Error('External activity summary is required for batch synchronization')
  const referenceVideo =
    hasExternalActivity && editorSnapshot.importedVideoPath !== null
      ? {
          path: editorSnapshot.importedVideoPath,
          creationTime: editorSnapshot.importedVideoCreationTime,
          timeSource: editorSnapshot.importedVideoTimeSource,
          committedOffsetSeconds: editorSnapshot.videoSyncOffsetSeconds,
        }
      : null
  const calibration = createBatchCalibration({ activitySummary, referenceVideo, timezoneMode: editorSnapshot.videoSyncTimezoneMode })
  return { activitySummary, calibration }
}

/**
 * Captures the native request from one synchronous editor/settings snapshot.
 * Rust owns destinations, descriptors, validation and correction application.
 * @param {object} options Editor snapshot, settings, inspectionId, outputDirectory, eligible jobs and optional calibrationSource.
 * @returns {Readonly<object>} Owned payload matching Rust BatchRenderRequest.
 */
export function createBatchRenderRequest({ editorSnapshot, settings, inspectionId, outputDirectory, jobs, calibrationSource }) {
  const { activitySummary, calibration } = captureBatchSync(editorSnapshot)
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
            jobs.map(({ source }) => [
              source.sourceId,
              resolveBatchVideoTiming(source.metadata, activitySummary, calibration, source.sourceId === calibrationSource?.sourceId).timing
                .automaticOffsetSeconds,
            ]),
          ),
        }
  return freezeRequest(
    structuredClone({
      inspectionId,
      template: createBatchRenderTemplate(editorSnapshot.config, editorSnapshot.globalDefaults),
      encoding: captureBatchEncoding(settings, editorSnapshot.availableCodecs),
      activity,
      outputDirectory,
      jobs: jobs.map(({ id, source, skipOverlay }) => ({ id, sourceId: source.sourceId, skipOverlay })),
    }),
  )
}
