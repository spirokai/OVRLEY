/**
 * Render-focused config preparation for backend video requests.
 *
 * This module starts from committed template state, materializes the
 * editor-effective template config, and then layers on render-only scene
 * fields such as codec defaults, export-range scoping, and optional
 * imported-video composite metadata.
 */

import { createEditorEffectiveConfig } from '@/lib/template/template-state'
import { SCENE_PRESENTATION_KEYS } from '@/lib/template/template-constants'
import { clamp } from '@/lib/utils'
import { isMp4Codec, isQsvFullCodec } from './codecUtils'

function renderValues(values) {
  return values.map(({ display_variants: _displayVariants, ...value }) => value)
}

/**
 * Materializes shared batch presentation once, excluding editor timing and
 * source/encoding fields. Each native video plan owns its dimensions and clock.
 * @param {object} config Committed template configuration.
 * @param {object} globalDefaults Captured global presentation defaults.
 * @param {string} language App language that names chart legends.
 * @returns {object} Canonical shared batch template.
 */
export function createBatchRenderTemplate(config, globalDefaults, language) {
  const effective = createEditorEffectiveConfig({ config, globalDefaults, language })
  // Editor-effective scenes also contain widget defaults. Project only the
  // native ScenePresentationConfig fields; widgets are already materialized.
  const scene = Object.fromEntries(SCENE_PRESENTATION_KEYS.map((key) => [key, effective.scene[key]]))
  return {
    scene,
    backdrops: effective.backdrops,
    rasters: effective.rasters,
    labels: effective.labels,
    values: renderValues(effective.values),
    plots: effective.plots,
  }
}

/**
 * Applies codec-specific FFmpeg defaults after the render codec is resolved.
 *
 * @param {object} scene - Render-effective scene config.
 * @param {string} resolvedExportCodec - Final codec used for the render job.
 */
function applyCodecDefaults(scene, resolvedExportCodec) {
  if (resolvedExportCodec === 'prores_ks') {
    scene.ffmpeg.prores_profile ??= '4444'
    scene.ffmpeg.pix_fmt ??= 'yuva444p10le'
    return
  }

  if (resolvedExportCodec === 'prores_ks_vulkan') {
    scene.ffmpeg.prores_profile ??= '4'
    scene.ffmpeg.alpha_bits ??= 16
    return
  }

  if (resolvedExportCodec === 'qtrle') {
    scene.ffmpeg.pix_fmt ??= 'argb'
  }
}

/**
 * Adds imported-video render fields that never belong in durable template state.
 *
 * @param {object} scene - Render-effective scene config.
 * @param {object} options - Render preparation options.
 */
function applyCompositeSceneFields(scene, options) {
  const { video, videoOffsetSeconds, qualityType, qualityValue } = options

  scene.width = video.resolution.width
  scene.height = video.resolution.height
  scene.composite_video_path = video.path
  scene.qualityType = qualityType
  scene.qualityValue = qualityValue
  scene.composite_sync_offset = videoOffsetSeconds
  scene.composite_video_fps_num = video.fps.num
  scene.composite_video_fps_den = video.fps.den
  scene.composite_video_duration = video.durationSeconds
  scene.composite_render_duration = video.durationSeconds
  scene.composite_video_trim_start = 0
  scene.composite_widget_update_rate = scene.update_rate
}

/**
 * Applies the custom export-range window to the render-effective scene config.
 *
 * Transparent exports narrow the activity window directly. Composite exports
 * translate the activity-timeline range into a video-local trim and duration.
 *
 * @param {object} scene - Render-effective scene config.
 * @param {object|null|undefined} exportRange - Requested export-range settings.
 * @param {boolean} isComposite - Whether an imported video is composited.
 */
function applyCustomExportRange(scene, exportRange, isComposite) {
  scene.custom_export_range_active = isComposite

  if (exportRange.type !== 'custom') {
    return
  }

  const start = exportRange.from
  const end = exportRange.to

  if (end <= start) {
    throw new Error('Custom export range end must be after its start.')
  }

  if (!isComposite) {
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
 * Applies the captured editor timeline, which owns activity-specific timing.
 *
 * Durable template state strips activity-specific timing, but the renderer
 * still requires an explicit scene window. The editor timeline remains the
 * source of truth for that session window, so render preparation restores it
 * here before any export-range overrides are applied.
 *
 * @param {object} scene - Render-effective scene config.
 * @param {number} timelineStart - Active editor timeline start second.
 * @param {number} timelineEnd - Active editor timeline end second.
 */
function applyTimelineSceneFields(scene, timelineStart, timelineEnd) {
  scene.start = timelineStart
  scene.end = timelineEnd
}

/**
 * Materializes the render-effective config sent to the backend.
 *
 * @param {object} options - Render preparation options.
 * @param {object|null|undefined} options.availableCodecs - Detected codec metadata from the backend.
 * @param {object} options.config - Committed template config.
 * @param {string} options.codec - Requested export codec.
 * @param {'quality'|'bitrate'} options.qualityType - Composite rate control mode.
 * @param {number} options.qualityValue - CRF value (1–51) or bitrate in Mbps.
 * @param {'transparent'|'composite'} options.exportMode - Active export pipeline selection.
 * @param {object} options.range - Validated export range settings.
 * @param {object|null|undefined} options.globalDefaults - Template global defaults.
 * @param {{path: string, durationSeconds: number, fps: {num: number, den: number}, resolution: {width: number, height: number}}|null} options.video - Composited video, if any.
 * @param {number} options.videoOffsetSeconds - Activity second where the composited video starts.
 * @param {number} options.timelineStart - Active editor timeline start second.
 * @param {number} options.timelineEnd - Active editor timeline end second.
 * @param {number[]} options.disabledLaps - Activity lap IDs excluded from lap widgets.
 * @param {number} options.fps - Validated overlay FPS.
 * @param {number} options.widgetUpdateRate - Validated widget update-rate divisor.
 * @param {string} options.language - App language that names chart legends.
 * @returns {object} Render-effective config.
 */
export function createRenderEffectiveConfig(options) {
  const {
    availableCodecs,
    config,
    codec,
    exportMode,
    range,
    globalDefaults,
    video,
    disabledLaps,
    timelineStart,
    timelineEnd,
    widgetUpdateRate,
    fps,
    language,
  } = options

  if (!config?.scene) {
    throw new Error('No valid config available')
  }

  const nextConfig = createEditorEffectiveConfig({ config, globalDefaults, language })
  const scene = {
    ...nextConfig.scene,
  }
  const shouldComposite = exportMode === 'composite'
  if (shouldComposite && video === null) throw new Error('Imported video is required for composite export')
  if (isMp4Codec(codec) !== shouldComposite) throw new Error('Render codec must match the export mode')

  scene.fps = fps
  delete scene.updateRate
  scene.update_rate = widgetUpdateRate
  scene.ffmpeg = {
    ...scene.ffmpeg,
    codec,
  }

  if (isQsvFullCodec(codec)) {
    scene.ffmpeg.qsv_full_init_args = availableCodecs.qsvFullInitArgs
  } else {
    delete scene.ffmpeg.qsv_full_init_args
  }

  if (shouldComposite) {
    applyCompositeSceneFields(scene, options)
  }

  applyTimelineSceneFields(scene, timelineStart, timelineEnd)
  scene.disabled_laps = disabledLaps
  applyCodecDefaults(scene, codec)
  // A video outside the activity renders with raster widgets only (native coverage).
  applyCustomExportRange(scene, range, shouldComposite)

  return {
    ...nextConfig,
    scene,
    values: renderValues(nextConfig.values),
  }
}
