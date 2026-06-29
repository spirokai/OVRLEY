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
    setActiveRenderId,
    setRenderingVideo,
    setRenderProgress,
    startSecond,
    endSecond,
    updateRate,
    videoSyncOffsetSeconds,
  } = overrides

  try {
    const activeConfig = overrides.config || baseConfig
    const activeUpdateRate = overrides.updateRate ?? updateRate
    const activeExportRange = overrides.exportRange ?? exportRange
    const activeExportMode = overrides.exportMode
    const activeExportCodec = overrides.exportCodec ?? exportCodec
    const activeExportBitrate = overrides.exportBitrate

    const config = createRenderEffectiveConfig({
      availableCodecs,
      config: activeConfig,
      exportBitrate: activeExportBitrate,
      exportCodec: activeExportCodec,
      exportMode: activeExportMode,
      exportRange: activeExportRange,
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

    if (config.scene.start === undefined || config.scene.end === undefined) {
      throw new Error('Timeline start and end must be set')
    }

    if (config.scene.start >= config.scene.end) {
      throw new Error('Start time must be before end time')
    }

    setRenderingVideo(true)
    setActiveRenderId(null)
    setRenderProgress({
      renderId: null,
      current: 0,
      total: 0,
      encoded: 0,
      status: 'rendering',
      message: 'Starting render...',
      estimatedSecondsRemaining: null,
      filename: null,
    })

    console.log('Sending video render request:', {
      start: config?.scene?.start,
      end: config?.scene?.end,
      duration: (config?.scene?.end || 0) - (config?.scene?.start || 0),
      updateRate: config?.scene?.update_rate,
    })

    const data = await backend.renderVideo(config, parsedActivity)

    if (data.error) {
      if (data.cancelled || data.error.toLowerCase().includes('cancelled')) {
        console.log('Render cancelled by user')
        return { success: false, cancelled: true }
      }

      throw new Error(data.error)
    }

    if (data.started) {
      setActiveRenderId(data.render_id ?? null)
      return { success: true, started: true }
    }

    throw new Error('Render did not start')
  } catch (error) {
    setActiveRenderId(null)
    setRenderingVideo(false)
    setRenderProgress({
      renderId: null,
      current: 0,
      total: 0,
      encoded: 0,
      status: 'error',
      message: error.message || 'Render failed to start',
      estimatedSecondsRemaining: null,
      filename: null,
    })
    console.error('Error in renderVideo:', error)
    throw error
  }
}
