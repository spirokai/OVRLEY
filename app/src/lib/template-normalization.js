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
 * - template-defaults.js    provides the constant default values
 * - template-state.js       orchestrates durable ↔ effective materialization
 *
 * @module template-normalization
 */

import { normalizeColorFields } from '@/lib/color-utils'
import { ensureWidgetIdsInConfig } from '@/lib/widget-config'
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
import { DEFAULT_GLOBAL_DEFAULTS, GLOBAL_DEFAULT_KEYS, SCENE_DERIVED_SETTING_KEYS, SCENE_GLOBAL_DEFAULT_KEYS } from './template-defaults'

function cloneSerializable(value) {
  if (value === undefined) return undefined
  return structuredClone(value)
}

function pickDefined(source, keys) {
  const result = {}
  if (!source) return result
  for (const key of keys) {
    if (source[key] !== undefined) result[key] = source[key]
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
  const sceneDefaults = pickDefined(scene, SCENE_GLOBAL_DEFAULT_KEYS)
  const mergedDefaults = { ...sceneDefaults, ...(cloneSerializable(globalDefaults) || {}) }
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
  for (const key of SCENE_DERIVED_SETTING_KEYS) delete nextScene[key]
  for (const key of SCENE_RENDER_TIME_ONLY_KEYS) delete nextScene[key]
  return normalizeColorFields(nextScene)
}

function normalizeLabel(label = {}) {
  const pickedLabel = pickDefined(label, LABEL_KEYS)
  return normalizeColorFields(pickedLabel)
}

function normalizeValue(value = {}) {
  const type = value.value
  const keys = [...VALUE_SHARED_KEYS, ...(VALUE_TYPE_KEYS[type] || VALUE_ICON_KEYS)]
  const withDefaults = { ...VALUE_DEFAULTS[type], ...value }
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
  if (fallbackFont) normalizedPointLabel.font = fallbackFont
  const explicitValues = pickDefined(pointLabel, ['font', 'font_size', 'color'])
  return normalizeColorFields({ ...normalizedPointLabel, ...explicitValues })
}

function normalizePlot(plot = {}, config, globalDefaults) {
  const type = plot.value
  const withDefaults = { ...PLOT_DEFAULTS[type], ...plot }
  if (type === 'elevation') {
    withDefaults.point_label = normalizePointLabel(plot.point_label, config, globalDefaults)
  }
  let keys = COURSE_PLOT_KEYS
  if (type === 'elevation') keys = ELEVATION_PLOT_KEYS
  if (type === 'heading') keys = HEADING_PLOT_KEYS
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
  const normalizedConfig = { scene: normalizeScene(nextConfig.scene), labels: [], values: [], plots: [] }
  if (Array.isArray(nextConfig.labels)) {
    for (const label of nextConfig.labels) normalizedConfig.labels.push(normalizeLabel(label))
  }
  if (Array.isArray(nextConfig.values)) {
    for (const value of nextConfig.values) normalizedConfig.values.push(normalizeValue(value))
  }
  if (Array.isArray(nextConfig.plots)) {
    for (const plot of nextConfig.plots) normalizedConfig.plots.push(normalizePlot(plot, nextConfig, globalDefaults))
  }
  return normalizedConfig
}

/**
 * Applies temporary preview-only overrides to already-effective widget data.
 *
 * @param {object} data - Effective widget data.
 * @param {object|null} previewOverrides - Ephemeral preview overrides.
 * @returns {object} Widget data including preview overrides.
 */
export function applyPreviewOverrides(data, previewOverrides) {
  if (!previewOverrides) return data
  return { ...data, ...previewOverrides }
}

/**
 * Copies only explicitly defined keys from a record.
 *
 * @param {object|null|undefined} source - Source record.
 * @param {string[]} keys - Keys to preserve when defined.
 * @returns {object} Picked object without undefined entries.
 */
export { pickDefined }
