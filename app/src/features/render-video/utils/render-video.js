/**
 * Orchestrates a video render request through the template-state seam and then
 * sends the prepared payload to the backend via Tauri IPC.
 */

import * as backend from '@/api/backend'
import { createRenderEffectiveConfig } from './renderConfig'

/**
 * Prepares and sends a video render request to the backend.
 *
 * `overrides.exportMode` decides whether imported-video metadata becomes a
 * composite render input or is ignored for transparent export.
 *
 * @param {object} [overrides={}] - Render settings overrides merged into effective config.
 * @returns {Promise<object>} Backend render response.
 */
export default async function renderVideo(overrides = {}) {
  const {
    availableCodecs,
    config: baseConfig,
    exportCodec,
    exportRange,
    globalDefaults,
    importedVideoDuration,
    importedVideoFps,
    importedVideoFpsDen,
    importedVideoFpsNum,
    importedVideoPath,
    importedVideoResolution,
    parsedActivity,
    startSecond,
    endSecond,
    updateRate,
    videoSyncOffsetSeconds,
    outputPath,
    overwrite = false,
  } = overrides

  const activeConfig = overrides.config || baseConfig
  const activeUpdateRate = overrides.updateRate ?? updateRate
  const activeExportMode = overrides.exportMode
  const activeExportCodec = overrides.exportCodec ?? exportCodec
  const activeExportBitrate = overrides.exportBitrate

  const config = createRenderEffectiveConfig({
    availableCodecs,
    config: activeConfig,
    exportBitrate: activeExportBitrate,
    exportCodec: activeExportCodec,
    exportMode: activeExportMode,
    exportRange: overrides.exportRange ?? exportRange,
    globalDefaults,
    importedVideoDuration,
    importedVideoFps,
    importedVideoFpsDen,
    importedVideoFpsNum,
    importedVideoPath,
    importedVideoResolution,
    timelineStart: startSecond,
    timelineEnd: endSecond,
    updateRate: activeUpdateRate,
    videoSyncOffsetSeconds,
  })

  if (!parsedActivity) {
    throw new Error('No parsed activity available')
  }

  if (!outputPath) {
    throw new Error('Render output target is required')
  }

  if (config.scene.start === undefined || config.scene.end === undefined) {
    throw new Error('Timeline start and end must be set')
  }

  if (config.scene.start >= config.scene.end) {
    throw new Error('Start time must be before end time')
  }

  const data = await backend.renderVideo(config, parsedActivity, {
    outputPath,
    overwrite,
  })

  if (data.started && data.render_id && data.outputPath) {
    return data
  }

  throw new Error('Render did not start')
}
