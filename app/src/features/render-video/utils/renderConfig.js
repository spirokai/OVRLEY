/**
 * Render-focused config preparation for backend video requests.
 *
 * This module starts from committed template state, materializes the
 * editor-effective template config, and then layers on render-only scene
 * fields such as codec defaults, export-range scoping, and optional
 * imported-video composite metadata.
 */

import { createEditorEffectiveConfig } from '@/lib/template/template-state'
import { normalizeUpdateRateForFps, sanitizeIntegerFps } from '@/lib/update-rate'
import { clamp } from '@/lib/utils'
import { videoOverlapsActivity } from '@/lib/video-timing'
import { isCompositeCodec, isQsvFullCodec, resolveCompositeFps } from './render-execution'

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
    importedVideoResolution,
    qualityType,
    qualityValue,
    videoSyncOffsetSeconds,
  } = options
  const sourceFps = resolveCompositeFps(importedVideoFpsNum, importedVideoFpsDen, importedVideoFps)
  const renderDuration = importedVideoDuration
  const displayWidth = importedVideoResolution?.width
  const displayHeight = importedVideoResolution?.height

  if (!sourceFps) {
    throw new Error('Imported video FPS is required for MP4 compositing.')
  }
  if (!Number.isFinite(renderDuration) || renderDuration <= 0) {
    throw new Error('Imported video duration is required for MP4 compositing.')
  }
  if (!Number.isFinite(displayWidth) || displayWidth <= 0 || !Number.isFinite(displayHeight) || displayHeight <= 0) {
    throw new Error('Imported video resolution is required for MP4 compositing.')
  }
  if (!Number.isFinite(videoSyncOffsetSeconds)) {
    throw new Error('Imported video sync offset must be a finite number.')
  }

  scene.width = displayWidth
  scene.height = displayHeight
  scene.composite_video_path = importedVideoPath
  scene.qualityType = qualityType
  scene.qualityValue = qualityValue
  scene.composite_sync_offset = videoSyncOffsetSeconds
  scene.composite_video_fps_num = sourceFps.num
  scene.composite_video_fps_den = sourceFps.den
  scene.composite_video_duration = renderDuration
  scene.composite_render_duration = renderDuration
  scene.composite_video_trim_start = 0
  scene.composite_widget_update_rate = scene.update_rate
}

/**
 * Validates the effective video/activity overlap after export-range translation.
 *
 * @param {object} scene - Render-effective scene config.
 * @param {number|null|undefined} timelineEnd - Activity timeline end.
 */
function validateCompositeTiming(scene, timelineEnd) {
  const syncOffset = scene.composite_sync_offset
  const renderDuration = scene.composite_render_duration
  const activityEnd = timelineEnd ?? Number.POSITIVE_INFINITY
  if (!videoOverlapsActivity({ videoStart: syncOffset, videoDuration: renderDuration, activityEnd })) {
    throw new Error('Imported video range must have positive overlap with the activity timeline.')
  }
}

/**
 * Applies the custom export-range window to the render-effective scene config.
 *
 * Transparent exports narrow the activity window directly. Composite exports
 * translate the activity-timeline range into a video-local trim and duration.
 *
 * @param {object} scene - Render-effective scene config.
 * @param {object|null|undefined} exportRange - Requested export-range settings.
 * @param {string|null|undefined} importedVideoPath - Active composite-video path, if any.
 */
function applyCustomExportRange(scene, exportRange, importedVideoPath) {
  scene.custom_export_range_active = Boolean(importedVideoPath)

  if (exportRange?.type !== 'custom') {
    return
  }

  const start = exportRange.from
  const end = exportRange.to

  if (!Number.isFinite(start) || !Number.isFinite(end)) {
    throw new Error('Custom export range must contain numeric start and end values.')
  }
  if (end <= start) {
    throw new Error('Custom export range end must be after its start.')
  }

  if (!importedVideoPath) {
    scene.start = start
    scene.end = end
    scene.custom_export_range_active = true
    return
  }

  const videoStart = scene.composite_sync_offset
  const videoEnd = videoStart + scene.composite_video_duration
  const clampedStart = clamp(start, videoStart, videoEnd)
  const clampedEnd = clamp(end, videoStart, videoEnd)
  if (clampedEnd <= clampedStart) {
    throw new Error('Custom export range must overlap the imported video range when exporting a composite video.')
  }

  scene.start = clampedStart
  scene.end = clampedEnd
  scene.custom_export_range_active = true
  scene.composite_video_trim_start = clampedStart - videoStart
  scene.composite_render_duration = clampedEnd - clampedStart
  scene.composite_sync_offset = clampedStart
}

/**
 * Rehydrates render-window timing from the editor timeline state when the
 * committed template config intentionally omits scene start/end.
 *
 * Durable template state strips activity-specific timing, but the renderer
 * still requires an explicit scene window. The editor timeline remains the
 * source of truth for that session window, so render preparation restores it
 * here before any export-range overrides are applied.
 *
 * @param {object} scene - Render-effective scene config.
 * @param {number|null|undefined} timelineStart - Active editor timeline start second.
 * @param {number|null|undefined} timelineEnd - Active editor timeline end second.
 */
function applyTimelineSceneFields(scene, timelineStart, timelineEnd) {
  const normalizedStart = Number(timelineStart)
  const normalizedEnd = Number(timelineEnd)

  if (scene.start === undefined && Number.isFinite(normalizedStart)) {
    scene.start = normalizedStart
  }

  if (scene.end === undefined && Number.isFinite(normalizedEnd)) {
    scene.end = normalizedEnd
  }
}

/**
 * Materializes the render-effective config sent to the backend.
 *
 * @param {object} options - Render preparation options.
 * @param {object|null|undefined} options.availableCodecs - Detected codec metadata from the backend.
 * @param {object} options.config - Committed template config.
 * @param {*} options.exportCodec - Requested export codec.
 * @param {'quality'|'bitrate'} options.qualityType - Composite rate control mode.
 * @param {number} options.qualityValue - CRF value (1–51) or bitrate in Mbps.
 * @param {'transparent'|'composite'|null|undefined} options.exportMode - Active export pipeline selection.
 * @param {object|null|undefined} options.exportRange - Export range settings.
 * @param {object|null|undefined} options.globalDefaults - Template global defaults.
 * @param {string|null|undefined} options.importedVideoPath - Imported-video path, if any.
 * @param {number|null|undefined} options.timelineStart - Active editor timeline start second.
 * @param {number|null|undefined} options.timelineEnd - Active editor timeline end second.
 * @param {*} options.updateRate - Requested widget update-rate divisor.
 * @returns {object} Render-effective config.
 */
export function createRenderEffectiveConfig(options) {
  const { availableCodecs, config, exportCodec, exportMode, exportRange, globalDefaults, importedVideoPath, timelineStart, timelineEnd, updateRate } =
    options

  if (!config?.scene) {
    throw new Error('No valid config available')
  }

  const nextConfig = createEditorEffectiveConfig({ config, globalDefaults })
  const scene = {
    ...nextConfig.scene,
  }
  // Callers that do not pass an explicit export mode still follow the existing
  // imported-video default of compositing; dialog callers can now opt out with
  // transparent mode.
  const shouldComposite = exportMode ? exportMode === 'composite' && Boolean(importedVideoPath) : Boolean(importedVideoPath)
  const resolvedExportCodec = shouldComposite && !isCompositeCodec(exportCodec) ? 'libx264' : exportCodec || 'prores_ks'

  scene.fps = sanitizeIntegerFps(scene.fps)
  delete scene.updateRate
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

  if (shouldComposite) {
    applyCompositeSceneFields(scene, options)
  }

  applyTimelineSceneFields(scene, timelineStart, timelineEnd)
  applyCodecDefaults(scene, resolvedExportCodec)
  applyCustomExportRange(scene, exportRange, shouldComposite ? importedVideoPath : null)
  if (shouldComposite) {
    validateCompositeTiming(scene, timelineEnd)
  }

  return {
    ...nextConfig,
    scene,
    values: nextConfig.values?.map(({ display_variants: _displayVariants, ...value }) => {
      if (value.display_type !== 'lean_angle') return value
      const { width: _width, height: _height, ...renderValue } = value
      return renderValue
    }),
  }
}
