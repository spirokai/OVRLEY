/**
 * @file template-normalization – Normalizes template config and global
 * defaults into durable (save/load) shapes.
 *
 * Every function here is a pure normalization step. They do NOT materialize
 * editor-effective config — that is the orchestrator's job (template-state.js).
 *
 * What this module owns:
 * - Durable global-default normalization (strip unknowns, fill defaults, normalize colors)
 * - Durable scene/label/value/plot widget normalization
 * - Merging legacy scene-owned globals into the settings block
 *
 * What the orchestrator (template-state.js) owns:
 * - Building editor-effective config from normalized globals + committed config
 * - Global-to-committed-config sync (pushing globals into widget data)
 * - Public API composition (createDurableTemplateState, createEditorEffectiveConfig, etc.)
 *
 * Sibling modules:
 * - template-state.js       orchestrates durable ↔ effective materialization
 *
 * @module template-normalization
 */

import { normalizeColorFields } from '@/lib/color-utils'
import { pickDefined } from '@/lib/utils'
import { WIDGET_TYPE_DEFINITIONS, DISPLAY_TYPE_DEFINITIONS, requireDisplayMode } from '@/lib/widget/widget-catalog'
import { ensureWidgetIdsInConfig } from '../widget/widget-config'
import { initDisplayVariant } from '../widget/widget-resolver'
import { DEFAULT_GLOBAL_DEFAULTS, SCENE_DURABLE_KEYS, SCENE_RENDER_TIME_ONLY_KEYS } from './template-constants'

function cloneSerializable(value) {
  if (value === undefined) return undefined
  return structuredClone(value)
}

/**
 * Normalizes global defaults to the durable template shape.
 *
 * The durable template contract stores only known global-default keys and
 * always materializes missing values from app defaults.
 *
 * @param {object|null|undefined} globalDefaults - Candidate global defaults.
 * @returns {object} Normalized durable global defaults.
 */
export function normalizeGlobalDefaults(globalDefaults) {
  const pickedDefaults = pickDefined(cloneSerializable(globalDefaults) || {}, Object.keys(DEFAULT_GLOBAL_DEFAULTS))
  const mergedDefaults = { ...DEFAULT_GLOBAL_DEFAULTS, ...pickedDefaults }
  return normalizeColorFields(mergedDefaults)
}

/**
 * Merges scene-owned durable defaults into explicit template settings.
 *
 * Scene style fields used to live partly on `scene` and partly in
 * `settings.globalDefaults`; this helper collapses that split for the durable
 * template state while letting explicit settings win.
 *
 * @param {object|null|undefined} scene - Durable scene config.
 * @param {object|null|undefined} globalDefaults - Explicit template settings.
 * @returns {object} Normalized durable global defaults.
 */
export function mergeSceneGlobalDefaults(scene, globalDefaults) {
  const sceneDefaults = pickDefined(scene, Object.keys(DEFAULT_GLOBAL_DEFAULTS))
  const mergedDefaults = { ...sceneDefaults, ...(cloneSerializable(globalDefaults) || {}) }
  return normalizeGlobalDefaults(mergedDefaults)
}

/**
 * Normalizes durable scene config for save/load.
 *
 * Only template-wide render defaults are persisted here. Scene timing
 * (`start`/`end`) belongs to the current activity/export session, not to the
 * reusable overlay template.
 *
 * @param {object} [scene={}] - Raw scene config.
 * @returns {object} Durable normalized scene config.
 */
function normalizeScene(scene = {}) {
  const sourceScene = cloneSerializable(scene) || {}
  const nextScene = pickDefined(sourceScene, SCENE_DURABLE_KEYS)
  const numericUpdateRate = Math.trunc(Number(sourceScene.updateRate))
  if (Number.isFinite(numericUpdateRate) && numericUpdateRate >= 1) {
    nextScene.updateRate = numericUpdateRate
  } else {
    delete nextScene.updateRate
  }
  for (const key of SCENE_RENDER_TIME_ONLY_KEYS) delete nextScene[key]
  return normalizeColorFields(nextScene)
}

function normalizeLabel(label = {}) {
  const pickedLabel = pickDefined(label, WIDGET_TYPE_DEFINITIONS.label.configKeys)
  return normalizeColorFields(pickedLabel)
}

function normalizeVariants(variants, definitions) {
  if (variants === undefined) return {}
  if (variants === null || typeof variants !== 'object' || Array.isArray(variants)) throw new Error('Display variants must be an object')
  const normalized = {}
  for (const [displayType, variantConfig] of Object.entries(variants)) {
    const definition = definitions[displayType]
    if (!definition) throw new Error(`Unknown display variant: ${displayType}`)
    if (variantConfig === null || typeof variantConfig !== 'object' || Array.isArray(variantConfig))
      throw new Error(`Invalid display variant: ${displayType}`)
    if (definition.variantKeys.length === 0) continue
    normalized[displayType] = normalizeColorFields(pickDefined(variantConfig, definition.variantKeys))
  }
  return normalized
}

function normalizeBackdrop(backdrop = {}) {
  const pickedBackdrop = normalizeColorFields(pickDefined(backdrop, WIDGET_TYPE_DEFINITIONS.backdrop.configKeys))
  const displayType = pickedBackdrop.display_type
  const allowedVariantKeys = DISPLAY_TYPE_DEFINITIONS[displayType].variantKeys
  const normalizedVariants = normalizeVariants(backdrop.display_variants, DISPLAY_TYPE_DEFINITIONS)

  if (allowedVariantKeys) {
    const promotedVariant = pickDefined(backdrop, allowedVariantKeys)
    if (Object.keys(promotedVariant).length > 0) {
      normalizedVariants[displayType] = normalizeColorFields({
        ...(normalizedVariants[displayType] || {}),
        ...promotedVariant,
      })
    }
  }

  if (Object.keys(normalizedVariants).length > 0) {
    pickedBackdrop.display_variants = normalizedVariants
  }

  return pickedBackdrop
}

function validateDisplayGeometry(value) {
  const displayType = value.display_type === undefined ? WIDGET_TYPE_DEFINITIONS[value.value].defaults.display_type : value.display_type
  const definition = DISPLAY_TYPE_DEFINITIONS[displayType]
  const variant = value.display_variants?.[displayType]
  for (const field of definition.forbiddenFrameFields) {
    if (Object.hasOwn(value, field) || (variant !== undefined && Object.hasOwn(variant, field))) {
      throw new Error(
        `${displayType} does not accept ${definition.forbiddenFrameFields.join(' or ')}; use ${definition.requiredPositiveFields.join(', ')}`,
      )
    }
  }
  for (const field of definition.requiredPositiveFields) {
    if (!Number.isFinite(variant?.[field]) || variant[field] <= 0) throw new Error(`${displayType} ${field} must be a positive finite number`)
  }
}

function normalizeLinearGaugeLabelPosition(variant) {
  if (!variant || typeof variant !== 'object') return variant

  const orientation = variant.orientation === 'vertical' ? 'vertical' : 'horizontal'
  const allowedPositions = orientation === 'vertical' ? ['left', 'right'] : ['bottom', 'top']
  const fallbackPosition = orientation === 'vertical' ? 'left' : 'bottom'
  const position = typeof variant.min_max_label_position === 'string' ? variant.min_max_label_position : fallbackPosition

  return {
    ...variant,
    orientation,
    min_max_label_position: allowedPositions.includes(position) ? position : fallbackPosition,
  }
}

function normalizeValue(value = {}, globalDefaults) {
  const type = value.value
  if (WIDGET_TYPE_DEFINITIONS[type]?.collection !== 'values') {
    throw new Error(`Unknown value widget type: ${String(type)}`)
  }

  validateDisplayGeometry(value)
  const withDefaults = { ...WIDGET_TYPE_DEFINITIONS[type].defaults, ...value }
  if (type === 'time') {
    if (!WIDGET_TYPE_DEFINITIONS.time.modes.some((mode) => mode.value === withDefaults.time_mode)) {
      throw new Error(`Invalid time_mode: ${String(withDefaults.time_mode)}`)
    }
    if (!WIDGET_TYPE_DEFINITIONS.time.elapsedOrigins.some((origin) => origin.value === withDefaults.elapsed_origin)) {
      throw new Error(`Invalid elapsed_origin: ${String(withDefaults.elapsed_origin)}`)
    }
    if (typeof withDefaults.show_hundredths !== 'boolean') {
      throw new Error(`Invalid show_hundredths: ${String(withDefaults.show_hundredths)}`)
    }
  }
  const supportsContentAlignment = WIDGET_TYPE_DEFINITIONS[type].contentAlignment
  if (supportsContentAlignment && !['left', 'center', 'right'].includes(withDefaults.content_alignment)) {
    throw new Error(`Invalid content_alignment: ${String(withDefaults.content_alignment)}`)
  }
  if (type === 'lap_timer') {
    const mode = requireDisplayMode(withDefaults.display_type, value.lap_timer_mode)
    if (value.label_font_size === undefined) withDefaults.label_font_size = mode.defaults.label_font_size
    if (typeof withDefaults.renumber_laps !== 'boolean') {
      throw new Error(`Invalid renumber_laps: ${String(withDefaults.renumber_laps)}`)
    }
  }
  if (type === 'lap_timer' || type === 'splits_run') {
    const display = DISPLAY_TYPE_DEFINITIONS[withDefaults.display_type]
    if (value.label_font === undefined) withDefaults.label_font = globalDefaults?.font_text || display.defaults.label_font
    if (value.label_color === undefined) withDefaults.label_color = globalDefaults?.color_text || display.defaults.label_color
  }
  const pickedValue = pickDefined(withDefaults, WIDGET_TYPE_DEFINITIONS[type].configKeys)
  if (!supportsContentAlignment) delete pickedValue.content_alignment
  if (typeof pickedValue.display_unit !== 'string') {
    delete pickedValue.display_unit
  }
  if (pickedValue.display_variants !== undefined)
    pickedValue.display_variants = normalizeVariants(pickedValue.display_variants, DISPLAY_TYPE_DEFINITIONS)
  if (DISPLAY_TYPE_DEFINITIONS[pickedValue.display_type].layoutMode === 'boxed') {
    const initializedValue = initDisplayVariant(pickedValue, pickedValue.display_type)
    if (initializedValue.display_variants) pickedValue.display_variants = initializedValue.display_variants
  }
  if (pickedValue.display_variants) {
    if (pickedValue.display_variants?.linear) {
      pickedValue.display_variants.linear = normalizeLinearGaugeLabelPosition(pickedValue.display_variants.linear)
    }
  }
  return normalizeColorFields(pickedValue)
}

function normalizePointLabel(pointLabel, config, globalDefaults) {
  const fallbackFont = globalDefaults?.font_values || config?.scene?.font
  const fallbackColor = pointLabel?.color || globalDefaults?.color_values || '#ffffff'
  const normalizedPointLabel = {
    font_size: pointLabel?.font_size ?? config?.scene?.font_size ?? 12.5,
    color: fallbackColor,
  }
  if (fallbackFont) normalizedPointLabel.font = fallbackFont
  const explicitValues = pickDefined(pointLabel, ['font', 'font_size', 'color'])
  return normalizeColorFields({ ...normalizedPointLabel, ...explicitValues })
}

function normalizePlot(plot = {}, config, globalDefaults) {
  const type = plot.value
  if (WIDGET_TYPE_DEFINITIONS[type]?.collection !== 'plots') throw new Error(`Unknown plot widget type: ${type}`)
  const withDefaults = { ...WIDGET_TYPE_DEFINITIONS[type].defaults, ...plot }
  if (type === 'elevation' || type === 'diving_profile') {
    withDefaults.point_label = normalizePointLabel(plot.point_label, config, globalDefaults)
  }
  const pickedPlot = pickDefined(withDefaults, WIDGET_TYPE_DEFINITIONS[type].configKeys)
  return normalizeColorFields(pickedPlot)
}

/**
 * Normalizes the durable widget config saved inside template files.
 *
 * @param {object} config - Validated editor config.
 * @param {object|null|undefined} globalDefaults - Durable global defaults used for plot-label fallback normalization.
 * @returns {object} Durable normalized template config.
 */
export function normalizeTemplateConfig(config, globalDefaults) {
  if (!config) throw new Error('Template config must be an object')
  const clonedConfig = cloneSerializable(config)
  const nextConfig = ensureWidgetIdsInConfig(clonedConfig)
  const scene = normalizeScene(nextConfig.scene)
  const normalizedConfig = { scene, backdrops: [], rasters: [], labels: [], values: [], plots: [] }
  if (Array.isArray(nextConfig.backdrops)) {
    for (const backdrop of nextConfig.backdrops) normalizedConfig.backdrops.push(normalizeBackdrop(backdrop))
  }
  for (const raster of nextConfig.rasters) {
    const normalizedRaster = pickDefined(raster, WIDGET_TYPE_DEFINITIONS.raster.configKeys)
    normalizedConfig.rasters.push(normalizedRaster)
  }
  if (Array.isArray(nextConfig.labels)) {
    for (const label of nextConfig.labels) normalizedConfig.labels.push(normalizeLabel(label))
  }
  if (Array.isArray(nextConfig.values)) {
    for (const value of nextConfig.values) normalizedConfig.values.push(normalizeValue(value, globalDefaults))
  }
  if (Array.isArray(nextConfig.plots)) {
    for (const plot of nextConfig.plots) normalizedConfig.plots.push(normalizePlot(plot, nextConfig, globalDefaults))
  }
  return normalizedConfig
}
