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

function captureEncoding(settings, availableCodecs) {
  const { exportMode, exportCodec, fps, updateRate, qualityType, qualityValue } = settings
  const qsvFullInitArgs = isQsvFullCodec(exportCodec) ? (availableCodecs?.qsvFullInitArgs ?? null) : null
  return { exportMode, exportCodec, fps, updateRate, qualityType, qualityValue, qsvFullInitArgs }
}

/**
 * Captures an owned, immutable batch payload from one editor/settings snapshot.
 * Materializes shared presentation once and resolves timing using the shared
 * calibration utility. The caller supplies ordered, eligible job plans; backend
 * ingress owns request, configuration, source and destination validation.
 *
 * @param {object} options Request capture inputs.
 * @param {object} options.editorSnapshot Editor state captured by the caller.
 * @param {object} options.settings Captured dialog settings.
 * @param {string} options.inspectionId Inspection session identity.
 * @param {string} options.outputDirectory Selected output directory.
 * @param {Array<{id: string, source: object, skipOverlay: boolean, outputPath: string}>} options.jobs Ordered job plans.
 * @param {object|null} options.calibrationSource Inspected reference source; null without an external-activity reference.
 * @returns {Readonly<object>} Owned, frozen payload matching Rust BatchRenderRequest.
 */
export function createBatchRenderRequest({ editorSnapshot, settings, inspectionId, outputDirectory, jobs, calibrationSource }) {
  const encoding = captureEncoding(settings, editorSnapshot.availableCodecs)
  const template = createBatchRenderTemplate(editorSnapshot.config, editorSnapshot.globalDefaults)
  const hasExternalActivity = editorSnapshot.parsedActivitySource === 'activity-file'
  const externalActivity = hasExternalActivity ? editorSnapshot.parsedActivity : null
  const activitySummary = hasExternalActivity ? editorSnapshot.activitySummary : null
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
  const capturedJobs = jobs.map(({ id, source, skipOverlay, outputPath }) => ({
    id,
    source,
    timing: resolveBatchVideoTiming(source.metadata, activitySummary, calibration).timing,
    skipOverlay,
    outputPath,
  }))
  return freezeRequest(
    structuredClone({ inspectionId, template, encoding, externalActivity, calibration, calibrationSource, outputDirectory, jobs: capturedJobs }),
  )
}
