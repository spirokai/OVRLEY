/**
 * Render-focused config preparation for backend video requests.
 *
 * This module starts from committed template state, materializes the
 * editor-effective template config, and then layers on render-only scene
 * fields such as codec defaults, the activity window, and optional
 * imported-video composite metadata.
 */

import { createEditorEffectiveConfig } from '@/lib/template/template-state'
import { SCENE_PRESENTATION_KEYS } from '@/lib/template/template-constants'
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
 * Applies the rendered activity window. Durable template state strips activity
 * timing, but the renderer requires an explicit scene window. A trimmed
 * activity also scopes the plotted route to the window; a composite always
 * scopes it, so a video outside the activity renders raster widgets only.
 *
 * @param {object} scene - Render-effective scene config.
 * @param {{startSecond: number, endSecond: number, isTrimmed: boolean}} activityWindow - Rendered activity window.
 * @param {boolean} isComposite - Whether an imported video is composited.
 */
function applyActivityWindow(scene, activityWindow, isComposite) {
  scene.start = activityWindow.startSecond
  scene.end = activityWindow.endSecond
  scene.custom_export_range_active = isComposite || activityWindow.isTrimmed
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
 * @param {object|null|undefined} options.globalDefaults - Template global defaults.
 * @param {{path: string, durationSeconds: number, fps: {num: number, den: number}, resolution: {width: number, height: number}}|null} options.video - Composited video, if any.
 * @param {number} options.videoOffsetSeconds - Activity second where the composited video starts.
 * @param {{startSecond: number, endSecond: number, isTrimmed: boolean}} options.activityWindow - Rendered activity window.
 * @param {number[]} options.disabledLaps - Activity lap IDs excluded from lap widgets.
 * @param {number} options.fps - Validated overlay FPS.
 * @param {number} options.widgetUpdateRate - Validated widget update-rate divisor.
 * @param {string} options.language - App language that names chart legends.
 * @returns {object} Render-effective config.
 */
export function createRenderEffectiveConfig(options) {
  const { availableCodecs, config, codec, exportMode, activityWindow, globalDefaults, video, disabledLaps, widgetUpdateRate, fps, language } = options

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

  applyActivityWindow(scene, activityWindow, shouldComposite)
  scene.disabled_laps = disabledLaps
  applyCodecDefaults(scene, codec)

  return {
    ...nextConfig,
    scene,
    values: renderValues(nextConfig.values),
  }
}
