/**
 * Provides shared config utils utilities for the app.
 */

import { createFontSelection, getFontFamilyName } from './fonts'
import { getThemeColor } from './theme'

const SCENE_STYLE_DEFAULTS = {
  border_thickness: 0,
  shadow_strength: 0,
  shadow_distance: 0,
}

export const SCENE_STYLE_KEYS = [
  'border_color',
  'border_thickness',
  'shadow_color',
  'shadow_strength',
  'shadow_distance',
]

/**
 * Applies preview overrides to resolved widget data.
 *
 * @param {*} data - Value for data.
 * @param {*} previewOverrides - Value for preview overrides.
 * @returns {object} Result produced by the helper.
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

/**
 * Resolves effective scene data without mutating the original config object.
 *
 * @param {*} sceneData - Value for scene data.
 * @param {*} globals - Global defaults merged into widgets.
 * @returns {*} Requested value or structure.
 */
export function getEffectiveSceneData(sceneData, globals) {
  if (!sceneData) {
    return sceneData
  }

  return {
    ...SCENE_STYLE_DEFAULTS,
    ...sceneData,
    scale: globals?.scale,
  }
}

/**
 * Resolves effective label widget data without cloning the full config tree.
 *
 * @param {*} widgetData - Value for widget data.
 * @param {*} globals - Global defaults merged into widgets.
 * @param {*} previewOverrides - Value for preview overrides.
 * @returns {object} Derived data structure for downstream use.
 */
export function getEffectiveLabelData(
  widgetData = {},
  globals,
  previewOverrides = null,
) {
  const font = widgetData.font || globals?.font_text
  const nextData = {
    ...widgetData,
  }

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

/**
 * Resolves effective metric widget data without cloning the full config tree.
 *
 * @param {*} widgetData - Value for widget data.
 * @param {*} globals - Global defaults merged into widgets.
 * @param {*} previewOverrides - Value for preview overrides.
 * @returns {object} Derived data structure for downstream use.
 */
export function getEffectiveValueData(
  widgetData = {},
  globals,
  previewOverrides = null,
) {
  const font = widgetData.font || globals?.font_values
  const nextData = {
    ...widgetData,
  }

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
  if (nextData.opacity === undefined) {
    nextData.opacity = globals?.opacity
  }

  return applyPreviewOverrides(nextData, previewOverrides)
}

/**
 * Resolves effective plot widget data without cloning the full config tree.
 *
 * @param {*} widgetData - Value for widget data.
 * @param {*} globals - Global defaults merged into widgets.
 * @param {*} previewOverrides - Value for preview overrides.
 * @returns {object} Derived data structure for downstream use.
 */
export function getEffectivePlotData(
  widgetData = {},
  globals,
  previewOverrides = null,
) {
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
      ...(!nextData.point_label?.font
        ? createFontSelection(globals.font_values)
        : {}),
    }
  }

  return applyPreviewOverrides(nextData, previewOverrides)
}

/**
 * Resolves effective widget data based on widget category.
 *
 * @param {*} widget - Widget definition being rendered or edited.
 * @param {*} globals - Global defaults merged into widgets.
 * @param {*} previewOverrides - Value for preview overrides.
 * @returns {*} Requested value or structure.
 */
export function getEffectiveWidgetData(
  widget,
  globals,
  previewOverrides = null,
) {
  if (!widget) {
    return widget
  }

  if (widget.category === 'labels') {
    return getEffectiveLabelData(widget.data, globals, previewOverrides)
  }

  if (widget.category === 'values') {
    return getEffectiveValueData(widget.data, globals, previewOverrides)
  }

  if (widget.category === 'plots') {
    return getEffectivePlotData(widget.data, globals, previewOverrides)
  }

  return applyPreviewOverrides({ ...widget.data }, previewOverrides)
}

/**
 * Applies global defaults.
 *
 * @param {*} config - Overlay template configuration data.
 * @param {*} globals - Global defaults merged into widgets.
 * @returns {*} Result produced by the helper.
 */
export function applyGlobalDefaults(config, globals) {
  if (!config || !globals) return config

  return {
    ...config,
    labels: Array.isArray(config.labels)
      ? config.labels.map((label) => getEffectiveLabelData(label, globals))
      : config.labels,
    values: Array.isArray(config.values)
      ? config.values.map((value) => getEffectiveValueData(value, globals))
      : config.values,
    plots: Array.isArray(config.plots)
      ? config.plots.map((plot) => getEffectivePlotData(plot, globals))
      : config.plots,
    scene: getEffectiveSceneData(config.scene, globals),
  }
}

/**
 * Synchronizes global defaults to config.
 *
 * @param {*} config - Overlay template configuration data.
 * @param {*} globals - Global defaults merged into widgets.
 * @param {*} changedKeys - Value for changed keys.
 * @returns {*} Result produced by the helper.
 */
export function syncGlobalDefaultsToConfig(
  config,
  globals,
  changedKeys = null,
) {
  if (!config || !globals) return config

  const changedKeySet = changedKeys ? new Set(changedKeys) : null
  const shouldApply = (key) => !changedKeySet || changedKeySet.has(key)
  const nextConfig = JSON.parse(JSON.stringify(config))

  if (nextConfig.scene) {
    SCENE_STYLE_KEYS.forEach((key) => {
      if (shouldApply(key)) {
        nextConfig.scene[key] = globals[key] ?? SCENE_STYLE_DEFAULTS[key] ?? 0
      }
    })
  }

  if (nextConfig.labels) {
    nextConfig.labels.forEach((label) => {
      if (shouldApply('font_text')) {
        Object.assign(label, createFontSelection(globals.font_text))
      }
      if (shouldApply('color_text')) {
        label.color = globals.color_text
      }
    })
  }

  if (nextConfig.values) {
    nextConfig.values.forEach((value) => {
      if (shouldApply('font_values')) {
        Object.assign(value, createFontSelection(globals.font_values))
      }
      if (shouldApply('color_values')) {
        value.color = globals.color_values
      }
      if (shouldApply('color_icons') && Object.hasOwn(value, 'icon_color')) {
        value.icon_color = globals.color_icons
      }
    })
  }

  if (nextConfig.plots) {
    nextConfig.plots.forEach((plot) => {
      if (plot.value === 'elevation' && shouldApply('font_values')) {
        plot.point_label = {
          ...(plot.point_label || {}),
          ...createFontSelection(globals.font_values),
        }
      }
      if (shouldApply('color_values') && Object.hasOwn(plot, 'color')) {
        plot.color = globals.color_values
      }
    })
  }

  if (nextConfig.scene && shouldApply('scale')) {
    nextConfig.scene.scale = globals.scale
  }

  return nextConfig
}
