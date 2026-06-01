/**
 * Render-focused config preparation for backend video requests.
 *
 * This module starts from committed template state, materializes the
 * editor-effective template config, and then layers on render-only scene
 * fields such as codec defaults, export-range scoping, and imported-video
 * composite metadata.
 */

import { timeToSeconds } from '@/features/overlay-editor/utils/exportRange'
import { createEditorEffectiveConfig } from '@/lib/template-state'
import { normalizeUpdateRateForFps, sanitizeIntegerFps } from '@/lib/update-rate'
import { formatCompositeBitrate, isCompositeCodec, isQsvFullCodec, resolveCompositeFps } from './render-execution'

/**
 * Applies codec-specific FFmpeg defaults after the render codec is resolved.
 *
 * @param {object} scene - Render-effective scene config.
 * @param {string} resolvedExportCodec - Final codec used for the render job.
 */
function applyCodecDefaults(scene, resolvedExportCodec) {
  if (resolvedExportCodec === 'prores_ks') {
    scene.ffmpeg.prores_profile = scene.ffmpeg.prores_profile || '4444'
    scene.ffmpeg.pix_fmt = scene.ffmpeg.pix_fmt || 'yuva444p10le'
    return
  }

  if (resolvedExportCodec === 'prores_ks_vulkan') {
    scene.ffmpeg.prores_profile = scene.ffmpeg.prores_profile || '4'
    scene.ffmpeg.alpha_bits = scene.ffmpeg.alpha_bits || 16
    return
  }

  if (resolvedExportCodec === 'qtrle') {
    scene.ffmpeg.pix_fmt = scene.ffmpeg.pix_fmt || 'argb'
  }
}

/**
 * Adds imported-video render fields that never belong in durable template state.
 *
 * @param {object} scene - Render-effective scene config.
 * @param {object} options - Render preparation options.
 */
function applyCompositeSceneFields(scene, options) {
  const {
    importedVideoDuration,
    importedVideoFps,
    importedVideoFpsNum,
    importedVideoFpsDen,
    importedVideoPath,
    exportBitrate,
    videoSyncOffsetSeconds,
  } = options
  const sourceFps = resolveCompositeFps(importedVideoFpsNum, importedVideoFpsDen, importedVideoFps)
  const renderDuration = Number(importedVideoDuration)

  if (!sourceFps) {
    throw new Error('Imported video FPS is required for MP4 compositing.')
  }
  if (!Number.isFinite(renderDuration) || renderDuration <= 0) {
    throw new Error('Imported video duration is required for MP4 compositing.')
  }

  scene.composite_video_path = importedVideoPath
  scene.composite_bitrate = formatCompositeBitrate(exportBitrate)
  scene.composite_sync_offset = Number.isFinite(Number(videoSyncOffsetSeconds)) ? Number(videoSyncOffsetSeconds) : 0
  scene.composite_video_fps_num = sourceFps.num
  scene.composite_video_fps_den = sourceFps.den
  scene.composite_video_duration = renderDuration
  scene.composite_render_duration = renderDuration
  scene.composite_video_trim_start = 0
  scene.composite_widget_update_rate = scene.update_rate
}

/**
 * Applies the custom export-range window to the render-effective scene config.
 *
 * @param {object} scene - Render-effective scene config.
 * @param {object|null|undefined} exportRange - Requested export-range settings.
 * @param {string|null|undefined} importedVideoPath - Imported-video path, if any.
 */
function applyCustomExportRange(scene, exportRange, importedVideoPath) {
  scene.custom_export_range_active = Boolean(importedVideoPath)

  if (importedVideoPath || exportRange?.type !== 'custom') {
    return
  }

  const start = Math.trunc(timeToSeconds(exportRange.fromTime))
  const end = Math.trunc(timeToSeconds(exportRange.toTime))

  if (end > start) {
    scene.start = start
    scene.end = end
    scene.custom_export_range_active = true
  }
}

/**
 * Materializes the render-effective config sent to the backend.
 *
 * @param {object} options - Render preparation options.
 * @param {object|null|undefined} options.availableCodecs - Detected codec metadata from the backend.
 * @param {object} options.config - Committed template config.
 * @param {*} options.exportCodec - Requested export codec.
 * @param {object|null|undefined} options.exportRange - Export range settings.
 * @param {object|null|undefined} options.globalDefaults - Template global defaults.
 * @param {string|null|undefined} options.importedVideoPath - Imported-video path, if any.
 * @param {*} options.updateRate - Requested widget update-rate divisor.
 * @returns {object} Render-effective config.
 */
export function createRenderEffectiveConfig(options) {
  const { availableCodecs, config, exportCodec, exportRange, globalDefaults, importedVideoPath, updateRate } = options

  if (!config?.scene) {
    throw new Error('No valid config available')
  }

  const nextConfig = createEditorEffectiveConfig({ config, globalDefaults })
  const scene = {
    ...nextConfig.scene,
  }
  const resolvedExportCodec = importedVideoPath && !isCompositeCodec(exportCodec) ? 'libx264' : exportCodec || 'prores_ks'

  scene.fps = sanitizeIntegerFps(scene.fps)
  scene.update_rate = normalizeUpdateRateForFps(scene.fps, updateRate ?? scene.updateRate)
  scene.ffmpeg = {
    ...(scene.ffmpeg || {}),
    codec: resolvedExportCodec,
  }

  if (isQsvFullCodec(resolvedExportCodec) && Array.isArray(availableCodecs?.qsvFullInitArgs)) {
    scene.ffmpeg.qsv_full_init_args = availableCodecs.qsvFullInitArgs
  } else {
    delete scene.ffmpeg.qsv_full_init_args
  }

  if (importedVideoPath) {
    applyCompositeSceneFields(scene, options)
  }

  applyCodecDefaults(scene, resolvedExportCodec)
  applyCustomExportRange(scene, exportRange, importedVideoPath)

  return {
    ...nextConfig,
    scene,
  }
}
