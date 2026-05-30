/**
 * Owns template-state materialization across the app.
 *
 * The seam distinguishes two template-owned shapes explicitly:
 * - durable template state for file save/load
 * - editor-effective config after applying global defaults
 */

import { normalizeColorFields } from '@/lib/color-utils'
import { createFontSelection, getFontFamilyName } from '@/lib/fonts'
import { getThemeColor } from '@/lib/theme'
import {
  COURSE_PLOT_KEYS,
  ELEVATION_PLOT_KEYS,
  HEADING_PLOT_KEYS,
  LABEL_KEYS,
  PLOT_DEFAULTS,
  SCENE_RENDER_TIME_ONLY_KEYS,
  VALUE_DEFAULTS,
  VALUE_ICON_KEYS,
  VALUE_SHARED_KEYS,
  VALUE_TYPE_KEYS,
} from '@/features/template-manager/data/templateConstants'
import { ensureWidgetIdsInConfig } from '@/lib/widget-config'

export const SCENE_STYLE_DEFAULTS = {
  border_color: '#000000',
  border_thickness: 0,
  shadow_color: '#000000',
  shadow_strength: 0,
  shadow_distance: 0,
}

const SCENE_STYLE_KEYS = Object.keys(SCENE_STYLE_DEFAULTS)

export const DEFAULT_GLOBAL_DEFAULTS = {
  font_values: 'Arial.ttf',
  font_text: 'Arial.ttf',
  color_values: '#ffffff',
  color_text: '#ffffff',
  color_icons: '#ffffff',
  color_units: '#ffffff',
  ...SCENE_STYLE_DEFAULTS,
  opacity: 1,
  scale: 1,
}

export const GLOBAL_DEFAULT_KEYS = Object.keys(DEFAULT_GLOBAL_DEFAULTS)
export const SCENE_GLOBAL_DEFAULT_KEYS = [...SCENE_STYLE_KEYS, 'opacity', 'scale']
export const SCENE_DERIVED_SETTING_KEYS = [...SCENE_GLOBAL_DEFAULT_KEYS, 'font', 'color', 'font_size']

/**
 * Clones plain serializable template data without JSON round-tripping.
 *
 * @param {*} value - Serializable value to clone.
 * @returns {*} Deep clone of the input.
 */
function cloneSerializable(value) {
  if (value === undefined) {
    return undefined
  }

  return structuredClone(value)
}

/**
 * Copies only explicitly defined keys from a record.
 *
 * @param {object|null|undefined} source - Source record.
 * @param {string[]} keys - Keys to preserve when defined.
 * @returns {object} Picked object without undefined entries.
 */
function pickDefined(source, keys) {
  const result = {}

  if (!source) {
    return result
  }

  for (const key of keys) {
    if (source[key] !== undefined) {
      result[key] = source[key]
    }
  }

  return result
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
  const pickedDefaults = pickDefined(cloneSerializable(globalDefaults) || {}, GLOBAL_DEFAULT_KEYS)
  const mergedDefaults = {
    ...DEFAULT_GLOBAL_DEFAULTS,
    ...pickedDefaults,
  }

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
function mergeSceneGlobalDefaults(scene, globalDefaults) {
  const sceneDefaults = pickDefined(scene, SCENE_GLOBAL_DEFAULT_KEYS)
  const mergedDefaults = {
    ...sceneDefaults,
    ...(cloneSerializable(globalDefaults) || {}),
  }

  return normalizeGlobalDefaults(mergedDefaults)
}

/**
 * Normalizes durable scene config for save/load.
 *
 * Derived editor fields and render-only fields are intentionally removed here
 * so the saved template contains only durable authoring state.
 *
 * @param {object} [scene={}] - Raw scene config.
 * @returns {object} Durable normalized scene config.
 */
function normalizeScene(scene = {}) {
  const nextScene = cloneSerializable(scene) || {}

  for (const key of SCENE_DERIVED_SETTING_KEYS) {
    delete nextScene[key]
  }

  for (const key of SCENE_RENDER_TIME_ONLY_KEYS) {
    delete nextScene[key]
  }

  return normalizeColorFields(nextScene)
}

function normalizeLabel(label = {}) {
  const pickedLabel = pickDefined(label, LABEL_KEYS)
  return normalizeColorFields(pickedLabel)
}

function normalizeValue(value = {}) {
  const type = value.value
  const keys = [...VALUE_SHARED_KEYS, ...(VALUE_TYPE_KEYS[type] || VALUE_ICON_KEYS)]
  const withDefaults = {
    ...VALUE_DEFAULTS[type],
    ...value,
  }
  const pickedValue = pickDefined(withDefaults, keys)

  return normalizeColorFields(pickedValue)
}

function normalizePointLabel(pointLabel, config, globalDefaults) {
  const fallbackFont = globalDefaults?.font_values || config?.scene?.font
  const fallbackColor = pointLabel?.color || globalDefaults?.color_values || '#ffffff'
  const normalizedPointLabel = {
    font_size: pointLabel?.font_size ?? config?.scene?.font_size ?? 12.5,
    color: fallbackColor,
  }

  if (fallbackFont) {
    normalizedPointLabel.font = fallbackFont
  }

  const explicitValues = pickDefined(pointLabel, ['font', 'font_size', 'color'])

  return normalizeColorFields({
    ...normalizedPointLabel,
    ...explicitValues,
  })
}

function normalizePlot(plot = {}, config, globalDefaults) {
  const type = plot.value
  const withDefaults = {
    ...PLOT_DEFAULTS[type],
    ...plot,
  }

  if (type === 'elevation') {
    withDefaults.point_label = normalizePointLabel(plot.point_label, config, globalDefaults)
  }

  let keys = COURSE_PLOT_KEYS
  if (type === 'elevation') {
    keys = ELEVATION_PLOT_KEYS
  }
  if (type === 'heading') {
    keys = HEADING_PLOT_KEYS
  }

  const pickedPlot = pickDefined(withDefaults, keys)
  return normalizeColorFields(pickedPlot)
}

/**
 * Normalizes the durable widget config saved inside template files.
 *
 * @param {object|null|undefined} config - Candidate template config.
 * @param {object|null|undefined} globalDefaults - Durable global defaults used for plot-label fallback normalization.
 * @returns {object} Durable normalized template config.
 */
export function normalizeTemplateConfig(config, globalDefaults) {
  const nextConfig = ensureWidgetIdsInConfig(cloneSerializable(config) || {})
  const normalizedConfig = {
    scene: normalizeScene(nextConfig.scene),
    labels: [],
    values: [],
    plots: [],
  }

  if (Array.isArray(nextConfig.labels)) {
    for (const label of nextConfig.labels) {
      normalizedConfig.labels.push(normalizeLabel(label))
    }
  }

  if (Array.isArray(nextConfig.values)) {
    for (const value of nextConfig.values) {
      normalizedConfig.values.push(normalizeValue(value))
    }
  }

  if (Array.isArray(nextConfig.plots)) {
    for (const plot of nextConfig.plots) {
      normalizedConfig.plots.push(normalizePlot(plot, nextConfig, globalDefaults))
    }
  }

  return normalizedConfig
}

/**
 * Materializes the durable template state used by save/load and dirty checks.
 *
 * @param {object} options - Template state inputs.
 * @param {object|null|undefined} options.config - Current committed template config.
 * @param {object|null|undefined} options.globalDefaults - Current template global defaults.
 * @returns {{config: object, settings: {globalDefaults: object}}} Durable template state.
 */
export function createDurableTemplateState({ config, globalDefaults }) {
  const nextGlobalDefaults = mergeSceneGlobalDefaults(config?.scene, globalDefaults)

  return {
    config: normalizeTemplateConfig(config, nextGlobalDefaults),
    settings: {
      globalDefaults: nextGlobalDefaults,
    },
  }
}

/**
 * Applies temporary preview-only overrides to already-effective widget data.
 *
 * @param {object} data - Effective widget data.
 * @param {object|null} previewOverrides - Ephemeral preview overrides.
 * @returns {object} Widget data including preview overrides.
 */
function applyPreviewOverrides(data, previewOverrides) {
  if (!previewOverrides) {
    return data
  }

  return {
    ...data,
    ...previewOverrides,
  }
}

function buildEffectiveSceneData(sceneData, globals) {
  if (!sceneData) {
    return sceneData
  }

  const globalSceneStyle = pickDefined(globals, SCENE_STYLE_KEYS)

  return {
    ...SCENE_STYLE_DEFAULTS,
    ...sceneData,
    ...globalSceneStyle,
    font: globals?.font_text || sceneData.font || 'Arial.ttf',
    color: globals?.color_text || sceneData.color || getThemeColor('ice'),
    font_size: sceneData.font_size ?? 30,
    opacity: globals?.opacity,
    scale: globals?.scale,
  }
}

function buildEffectiveLabelData(widgetData = {}, globals, previewOverrides = null) {
  const nextData = {
    ...widgetData,
  }
  const font = widgetData.font || globals?.font_text

  if (!nextData.font && font) {
    nextData.font = font
  }
  if (!nextData.font_family) {
    nextData.font_family = getFontFamilyName(font || nextData.font_family)
  }
  if (!nextData.color) {
    nextData.color = globals?.color_text || getThemeColor('ice')
  }
  if (nextData.opacity === undefined) {
    nextData.opacity = globals?.opacity
  }

  return applyPreviewOverrides(nextData, previewOverrides)
}

function buildEffectiveValueData(widgetData = {}, globals, previewOverrides = null) {
  const nextData = {
    ...widgetData,
  }
  const font = widgetData.font || globals?.font_values

  if (!nextData.font && font) {
    nextData.font = font
  }
  if (!nextData.font_family) {
    nextData.font_family = getFontFamilyName(font || nextData.font_family)
  }
  if (!nextData.color) {
    nextData.color = globals?.color_values || getThemeColor('ice')
  }
  if (nextData.icon_color === undefined) {
    nextData.icon_color = globals?.color_icons || getThemeColor('aqua')
  }
  if (nextData.unit_color === undefined && widgetData.value !== 'time') {
    nextData.unit_color = globals?.color_units || '#ffffff'
  }
  if (nextData.opacity === undefined) {
    nextData.opacity = globals?.opacity
  }
  if (nextData.value === 'left_right_balance' && nextData.balance_format === undefined) {
    nextData.balance_format = 'percent_label'
  }

  return applyPreviewOverrides(nextData, previewOverrides)
}

function buildEffectivePlotData(widgetData = {}, globals, previewOverrides = null) {
  const nextData = {
    ...widgetData,
  }

  if (!nextData.color) {
    nextData.color = globals?.color_values || getThemeColor('ice')
  }
  if (nextData.opacity === undefined) {
    nextData.opacity = globals?.opacity
  }
  if (nextData.value === 'elevation' && globals?.font_values) {
    nextData.point_label = {
      ...(nextData.point_label || {}),
      ...(!nextData.point_label?.font ? createFontSelection(globals.font_values) : {}),
    }
  }

  return applyPreviewOverrides(nextData, previewOverrides)
}

/**
 * Resolves a single widget to its editor-effective representation.
 *
 * @param {object|null|undefined} widget - Widget wrapper with category/data fields.
 * @param {object|null|undefined} globals - Normalized global defaults.
 * @param {object|null} [previewOverrides=null] - Preview-only overrides.
 * @returns {object|null|undefined} Effective widget data.
 */
export function getEffectiveWidgetData(widget, globals, previewOverrides = null) {
  if (!widget) {
    return widget
  }

  if (widget.category === 'labels') {
    return buildEffectiveLabelData(widget.data, globals, previewOverrides)
  }

  if (widget.category === 'values') {
    return buildEffectiveValueData(widget.data, globals, previewOverrides)
  }

  if (widget.category === 'plots') {
    return buildEffectivePlotData(widget.data, globals, previewOverrides)
  }

  return applyPreviewOverrides({ ...widget.data }, previewOverrides)
}

/**
 * Materializes the editor-effective config from committed template state.
 *
 * This keeps the committed config durable while giving editing surfaces the
 * merged fonts, colors, opacity, scale, and scene style they need.
 *
 * @param {object} options - Template config inputs.
 * @param {object|null|undefined} options.config - Committed template config.
 * @param {object|null|undefined} options.globalDefaults - Template global defaults.
 * @returns {object|null|undefined} Editor-effective config.
 */
export function createEditorEffectiveConfig({ config, globalDefaults }) {
  if (!config) {
    return config
  }

  const normalizedGlobals = normalizeGlobalDefaults(globalDefaults)
  const nextConfig = {
    ...config,
    scene: buildEffectiveSceneData(config.scene, normalizedGlobals),
    labels: config.labels,
    values: config.values,
    plots: config.plots,
  }

  if (Array.isArray(config.labels)) {
    nextConfig.labels = []
    for (const label of config.labels) {
      nextConfig.labels.push(buildEffectiveLabelData(label, normalizedGlobals))
    }
  }

  if (Array.isArray(config.values)) {
    nextConfig.values = []
    for (const value of config.values) {
      nextConfig.values.push(buildEffectiveValueData(value, normalizedGlobals))
    }
  }

  if (Array.isArray(config.plots)) {
    nextConfig.plots = []
    for (const plot of config.plots) {
      nextConfig.plots.push(buildEffectivePlotData(plot, normalizedGlobals))
    }
  }

  return nextConfig
}

/**
 * Backward-compatible alias for editor-effective config materialization.
 *
 * @param {object|null|undefined} config - Committed template config.
 * @param {object|null|undefined} globalDefaults - Template global defaults.
 * @returns {object|null|undefined} Editor-effective config.
 */
export function applyGlobalDefaults(config, globalDefaults) {
  return createEditorEffectiveConfig({ config, globalDefaults })
}

/**
 * Pushes changed global defaults into the committed widget config.
 *
 * The store still uses this when global-default edits should immediately
 * rewrite widget-owned durable fields such as font and explicit colors.
 *
 * @param {object|null|undefined} config - Committed template config.
 * @param {object|null|undefined} globals - Normalized global defaults.
 * @param {string[]|null} [changedKeys=null] - Optional subset of changed default keys.
 * @returns {object|null|undefined} Updated committed config.
 */
export function syncGlobalDefaultsToConfig(config, globals, changedKeys = null) {
  if (!config || !globals) {
    return config
  }

  const changedKeySet = changedKeys ? new Set(changedKeys) : null
  const shouldApply = (key) => !changedKeySet || changedKeySet.has(key)
  const nextConfig = JSON.parse(JSON.stringify(config))

  if (nextConfig.labels) {
    for (const label of nextConfig.labels) {
      if (shouldApply('font_text')) {
        Object.assign(label, createFontSelection(globals.font_text))
      }
      if (shouldApply('color_text')) {
        label.color = globals.color_text
      }
    }
  }

  if (nextConfig.values) {
    for (const value of nextConfig.values) {
      if (shouldApply('font_values')) {
        Object.assign(value, createFontSelection(globals.font_values))
      }
      if (shouldApply('color_values')) {
        value.color = globals.color_values
      }
      if (shouldApply('color_icons') && Object.hasOwn(value, 'icon_color')) {
        value.icon_color = globals.color_icons
      }
      if (shouldApply('color_units') && value.value !== 'time') {
        value.unit_color = globals.color_units
      }
    }
  }

  if (nextConfig.plots) {
    for (const plot of nextConfig.plots) {
      if (plot.value === 'elevation' && shouldApply('font_values')) {
        plot.point_label = {
          ...(plot.point_label || {}),
          ...createFontSelection(globals.font_values),
        }
      }
      if (shouldApply('color_values') && Object.hasOwn(plot, 'color')) {
        plot.color = globals.color_values
      }
    }
  }

  return nextConfig
}
