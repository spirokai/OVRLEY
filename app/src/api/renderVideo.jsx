/**
 * Implements API helpers for render video.
 */

import { getCurrentParsedActivity } from './activityCache'
import useStore from '../store/useStore'
import * as backend from './backend'
import { applyGlobalDefaults } from '../lib/config-utils'
import { timeToSeconds } from '../lib/export-range'
import {
  normalizeUpdateRateForFps,
  sanitizeIntegerFps,
} from '../lib/update-rate'

/**
 * Renders video.
 *
 * @param {*} overrides - Value for overrides.
 * @returns {Promise<object>} Promise resolving to the operation result.
 */
export default async function renderVideo(overrides = {}) {
  try {
    const {
      config: baseConfig,
      globalDefaults,
      updateRate,
      exportRange,
      exportCodec,
      setActiveRenderId,
      setRenderingVideo,
      setRenderProgress,
    } = useStore.getState()
    const overrideConfig = overrides.config
    const overrideUpdateRate = overrides.updateRate
    const overrideExportRange = overrides.exportRange
    const overrideExportCodec = overrides.exportCodec

    const parsedActivity = getCurrentParsedActivity()
    const activeConfig = overrideConfig || baseConfig
    const activeUpdateRate = overrideUpdateRate ?? updateRate
    const activeExportRange = overrideExportRange ?? exportRange
    const activeExportCodec = overrideExportCodec ?? exportCodec

    // Validate we have required data
    if (!activeConfig || !activeConfig.scene) {
      throw new Error('No valid config available')
    }

    // Apply global defaults and overrides
    const config = applyGlobalDefaults(activeConfig, globalDefaults)

    // Apply performance and range overrides
    if (config.scene) {
      config.scene.fps = sanitizeIntegerFps(config.scene.fps)
      config.scene.update_rate = normalizeUpdateRateForFps(
        config.scene.fps,
        activeUpdateRate,
      )
      config.scene.ffmpeg = {
        ...(config.scene.ffmpeg || {}),
        codec: activeExportCodec || 'prores_ks',
      }

      if ((activeExportCodec || 'prores_ks') === 'prores_ks') {
        config.scene.ffmpeg.prores_profile =
          config.scene.ffmpeg.prores_profile || '4444'
        config.scene.ffmpeg.pix_fmt =
          config.scene.ffmpeg.pix_fmt || 'yuva444p10le'
      } else if ((activeExportCodec || 'prores_ks') === 'prores_ks_vulkan') {
        config.scene.ffmpeg.prores_profile =
          config.scene.ffmpeg.prores_profile || '4'
        config.scene.ffmpeg.alpha_bits = config.scene.ffmpeg.alpha_bits || 16
      } else if ((activeExportCodec || 'prores_ks') === 'qtrle') {
        config.scene.ffmpeg.pix_fmt = config.scene.ffmpeg.pix_fmt || 'argb'
      }

      // Apply export range override if custom
      config.scene.custom_export_range_active = false
      if (activeExportRange.type === 'custom') {
        const start = Math.trunc(timeToSeconds(activeExportRange.fromTime))
        const end = Math.trunc(timeToSeconds(activeExportRange.toTime))
        if (end > start) {
          config.scene.start = start
          config.scene.end = end
          config.scene.custom_export_range_active = true
        }
      }
    }

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

    console.log('📤 Sending video render request:', {
      start: config?.scene?.start,
      end: config?.scene?.end,
      duration: (config?.scene?.end || 0) - (config?.scene?.start || 0),
      updateRate: config?.scene?.update_rate,
    })

    const data = await backend.renderVideo(config, parsedActivity)

    if (data.error) {
      // Check for cancellation
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
    const { setActiveRenderId, setRenderingVideo, setRenderProgress } =
      useStore.getState()
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
