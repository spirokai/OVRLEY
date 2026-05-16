/**
 * Orchestrates a video render request — reads store state, applies config
 * transformations (global defaults, codec, export range, composite video),
 * and sends the payload to the backend via Tauri IPC.
 */

import { getCurrentParsedActivity } from '@/lib/activity/cache'
import useStore from '@/store/useStore'
import * as backend from '@/api/backend'
import { applyGlobalDefaults } from '@/lib/config-utils'
import { timeToSeconds } from '@/features/overlay-editor/utils/exportRange'
import { normalizeUpdateRateForFps, sanitizeIntegerFps } from '@/lib/update-rate'
import { formatCompositeBitrate, isCompositeCodec, isQsvFullCodec, resolveCompositeFps } from './render-execution'

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
      availableCodecs,
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

    if (!activeConfig || !activeConfig.scene) {
      throw new Error('No valid config available')
    }

    const config = applyGlobalDefaults(activeConfig, globalDefaults)

    if (config.scene) {
      config.scene.fps = sanitizeIntegerFps(config.scene.fps)
      config.scene.update_rate = normalizeUpdateRateForFps(config.scene.fps, activeUpdateRate)
      config.scene.ffmpeg = {
        ...(config.scene.ffmpeg || {}),
        codec: resolvedExportCodec,
      }
      if (isQsvFullCodec(resolvedExportCodec) && Array.isArray(availableCodecs?.qsvFullInitArgs)) {
        config.scene.ffmpeg.qsv_full_init_args = availableCodecs.qsvFullInitArgs
      } else {
        delete config.scene.ffmpeg.qsv_full_init_args
      }

      const isCompositeRender = Boolean(importedVideoPath)

      if (isCompositeRender) {
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

      config.scene.custom_export_range_active = isCompositeRender
      if (!isCompositeRender && activeExportRange.type === 'custom') {
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
