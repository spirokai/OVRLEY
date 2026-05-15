/**
 * Implements API helpers for render video.
 */

import { getCurrentParsedActivity } from './activityCache'
import useStore from '../store/useStore'
import * as backend from './backend'
import { applyGlobalDefaults } from '../lib/config-utils'
import { timeToSeconds } from '@/features/overlay-editor/utils/exportRange'
import { normalizeUpdateRateForFps, sanitizeIntegerFps } from '../lib/update-rate'

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
      importedVideoDuration,
      importedVideoFps,
      importedVideoFpsNum,
      importedVideoFpsDen,
      importedVideoPath,
      videoSyncOffsetSeconds,
      setActiveRenderId,
      setRenderingVideo,
      setRenderProgress,
    } = useStore.getState()
    const overrideConfig = overrides.config
    const overrideUpdateRate = overrides.updateRate
    const overrideExportRange = overrides.exportRange
    const overrideExportCodec = overrides.exportCodec
    const overrideExportBitrate = overrides.exportBitrate

    const parsedActivity = getCurrentParsedActivity()
    const activeConfig = overrideConfig || baseConfig
    const activeUpdateRate = overrideUpdateRate ?? updateRate
    const activeExportRange = overrideExportRange ?? exportRange
    const activeExportCodec = overrideExportCodec ?? exportCodec
    const resolvedExportCodec = importedVideoPath && !isCompositeCodec(activeExportCodec) ? 'libx264' : activeExportCodec || 'prores_ks'

    // Validate we have required data
    if (!activeConfig || !activeConfig.scene) {
      throw new Error('No valid config available')
    }

    // Apply global defaults and overrides
    const config = applyGlobalDefaults(activeConfig, globalDefaults)

    // Apply performance and range overrides
    if (config.scene) {
      config.scene.fps = sanitizeIntegerFps(config.scene.fps)
      config.scene.update_rate = normalizeUpdateRateForFps(config.scene.fps, activeUpdateRate)
      config.scene.ffmpeg = {
        ...(config.scene.ffmpeg || {}),
        codec: resolvedExportCodec,
      }

      if (importedVideoPath) {
        const sourceFps = resolveCompositeFps(importedVideoFpsNum, importedVideoFpsDen, importedVideoFps)
        const renderDuration = Number(importedVideoDuration)
        if (!sourceFps) {
          throw new Error('Imported video FPS is required for MP4 compositing.')
        }
        if (!Number.isFinite(renderDuration) || renderDuration <= 0) {
          throw new Error('Imported video duration is required for MP4 compositing.')
        }

        config.scene.composite_video_path = importedVideoPath
        config.scene.composite_bitrate = formatCompositeBitrate(overrideExportBitrate)
        config.scene.composite_sync_offset = Number.isFinite(Number(videoSyncOffsetSeconds)) ? Number(videoSyncOffsetSeconds) : 0
        config.scene.composite_video_fps_num = sourceFps.num
        config.scene.composite_video_fps_den = sourceFps.den
        config.scene.composite_video_duration = renderDuration
        config.scene.composite_render_duration = renderDuration
        config.scene.composite_video_trim_start = 0
        config.scene.composite_widget_update_rate = config.scene.update_rate
      }

      if (resolvedExportCodec === 'prores_ks') {
        config.scene.ffmpeg.prores_profile = config.scene.ffmpeg.prores_profile || '4444'
        config.scene.ffmpeg.pix_fmt = config.scene.ffmpeg.pix_fmt || 'yuva444p10le'
      } else if (resolvedExportCodec === 'prores_ks_vulkan') {
        config.scene.ffmpeg.prores_profile = config.scene.ffmpeg.prores_profile || '4'
        config.scene.ffmpeg.alpha_bits = config.scene.ffmpeg.alpha_bits || 16
      } else if (resolvedExportCodec === 'qtrle') {
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
    const { setActiveRenderId, setRenderingVideo, setRenderProgress } = useStore.getState()
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

/**
 * Resolves imported-video FPS metadata to the rational fields expected by Rust.
 *
 * @param {*} fpsNum - Exact FPS numerator from ffprobe, when available.
 * @param {*} fpsDen - Exact FPS denominator from ffprobe, when available.
 * @param {*} fps - Floating FPS fallback from older metadata.
 * @returns {{num:number, den:number}|null} Reduced rational FPS or null.
 */
function resolveCompositeFps(fpsNum, fpsDen, fps) {
  const num = Number(fpsNum)
  const den = Number(fpsDen)
  if (Number.isInteger(num) && num > 0 && Number.isInteger(den) && den > 0) {
    return reduceFps(num, den)
  }

  const value = Number(fps)
  if (!Number.isFinite(value) || value <= 0) {
    return null
  }

  const commonRates = [
    [23.976, 24000, 1001],
    [29.97, 30000, 1001],
    [59.94, 60000, 1001],
    [25, 25, 1],
    [30, 30, 1],
    [60, 60, 1],
  ]
  const match = commonRates.find(([approx]) => Math.abs(value - approx) <= 0.001)
  if (match) {
    return { num: match[1], den: match[2] }
  }

  return reduceFps(Math.round(value * 1000), 1000)
}

/**
 * Reduces a rational FPS pair.
 *
 * @param {number} num - FPS numerator.
 * @param {number} den - FPS denominator.
 * @returns {{num:number, den:number}} Reduced FPS pair.
 */
function reduceFps(num, den) {
  let a = Math.abs(num)
  let b = Math.abs(den)
  while (b !== 0) {
    const next = a % b
    a = b
    b = next
  }
  const gcd = Math.max(a, 1)
  return { num: num / gcd, den: den / gcd }
}

/**
 * Formats dialog bitrate values for FFmpeg's `-b:v` argument.
 *
 * @param {*} value - Numeric Mbps value or already formatted FFmpeg bitrate.
 * @returns {string} FFmpeg bitrate string.
 */
function formatCompositeBitrate(value) {
  const bitrate = Number(value)
  if (Number.isFinite(bitrate) && bitrate > 0) {
    return `${bitrate}M`
  }
  if (typeof value === 'string' && value.trim()) {
    return value.trim()
  }
  return '20M'
}

/**
 * Returns whether a codec is valid for MP4 compositing output.
 *
 * @param {*} codec - Candidate FFmpeg codec name.
 * @returns {boolean} Whether the codec belongs to the composite MP4 path.
 */
function isCompositeCodec(codec) {
  return [
    'libx264',
    'libx265',
    'h264_nvenc',
    'hevc_nvenc',
    'h264_qsv',
    'hevc_qsv',
    'h264_amf',
    'hevc_amf',
    'h264_videotoolbox',
    'hevc_videotoolbox',
    'h264_vaapi',
    'hevc_vaapi',
  ].includes(codec)
}
