/**
 * @file template-state – Orchestration layer for durable ↔ editor-effective
 * template materialization.
 *
 * This module composes the normalization layer (template-normalization.js) to
 * produce the two public shapes the rest of the app depends on:
 *
 * 1. **Durable template state** – for file save/load and dirty checks.
 *    createDurableTemplateState normalizes the config and settings into a
 *    serialization-ready shape (derived/transient keys stripped).
 *
 * 2. **Editor-effective config** – for in-app editing surfaces.
 *    createEditorEffectiveConfig merges global defaults (fonts, colors,
 *    opacity, scale) into the committed config so editors see fully resolved
 *    widget data without having to resolve globals themselves.
 *
 * Callers should import from this module only — re-exports keep a single
 * import path.
 *
 * @module template-state
 */

import { createFontSelection, getFontFamilyName } from '@/lib/fonts'
import { getThemeColor } from '@/lib/theme'
import { resolveActiveMetricWidgetData } from '../widget/metric-widget-resolver'
import { DEFAULT_GLOBAL_DEFAULTS } from './template-constants'
import {
  applyPreviewOverrides,
  mergeSceneGlobalDefaults,
  normalizeGlobalDefaults,
  normalizeTemplateConfig,
  pickDefined,
} from './template-normalization'

function buildEffectiveSceneData(sceneData, globals) {
  if (!sceneData) return sceneData
  return {
    ...DEFAULT_GLOBAL_DEFAULTS,
    ...sceneData,
    ...pickDefined(globals, Object.keys(DEFAULT_GLOBAL_DEFAULTS)),
  }
}

function buildEffectiveLabelData(widgetData = {}, globals, previewOverrides = null) {
  const nextData = { ...widgetData }
  const font = widgetData.font || globals?.font_text
  if (!nextData.font && font) nextData.font = font
  if (!nextData.font_family) nextData.font_family = getFontFamilyName(font || nextData.font_family)
  if (!nextData.color) nextData.color = globals?.color_text || getThemeColor('ice')
  if (nextData.opacity === undefined) nextData.opacity = globals?.opacity
  return applyPreviewOverrides(nextData, previewOverrides)
}

function buildEffectiveValueData(widgetData = {}, globals, previewOverrides = null) {
  // Resolve display_variants for non-text display types
  const resolved = resolveActiveMetricWidgetData(widgetData)

  const nextData = { ...resolved }
  const font = resolved.font || globals?.font_values
  if (!nextData.font && font) nextData.font = font
  if (!nextData.font_family) nextData.font_family = getFontFamilyName(font || nextData.font_family)
  if (!nextData.color) nextData.color = globals?.color_values || getThemeColor('ice')
  if (nextData.icon_color === undefined) nextData.icon_color = globals?.color_icons || getThemeColor('aqua')
  if (nextData.unit_color === undefined && resolved.value !== 'time') nextData.unit_color = globals?.color_units || '#ffffff'
  if (nextData.opacity === undefined) nextData.opacity = globals?.opacity
  return applyPreviewOverrides(nextData, previewOverrides)
}

function buildEffectivePlotData(widgetData = {}, globals, previewOverrides = null) {
  const nextData = { ...widgetData }
  if (!nextData.color) nextData.color = globals?.color_values || getThemeColor('ice')
  if (nextData.opacity === undefined) nextData.opacity = globals?.opacity
  if (nextData.value === 'elevation' && globals?.font_values) {
    nextData.point_label = {
      ...(nextData.point_label || {}),
      ...(!nextData.point_label?.font ? createFontSelection(globals.font_values) : {}),
    }
  }
  return applyPreviewOverrides(nextData, previewOverrides)
}

export function getEffectiveWidgetData(widget, globals, previewOverrides = null) {
  if (!widget) return widget
  if (widget.category === 'labels') return buildEffectiveLabelData(widget.data, globals, previewOverrides)
  if (widget.category === 'values') return buildEffectiveValueData(widget.data, globals, previewOverrides)
  if (widget.category === 'plots') return buildEffectivePlotData(widget.data, globals, previewOverrides)
  return applyPreviewOverrides({ ...widget.data }, previewOverrides)
}

export function createDurableTemplateState({ config, globalDefaults }) {
  const nextGlobalDefaults = mergeSceneGlobalDefaults(config?.scene, globalDefaults)
  return {
    config: normalizeTemplateConfig(config, nextGlobalDefaults),
    settings: { globalDefaults: nextGlobalDefaults },
  }
}

export function createEditorEffectiveConfig({ config, globalDefaults }) {
  if (!config) return config
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
    for (const label of config.labels) nextConfig.labels.push(buildEffectiveLabelData(label, normalizedGlobals))
  }
  if (Array.isArray(config.values)) {
    nextConfig.values = []
    for (const value of config.values) nextConfig.values.push(buildEffectiveValueData(value, normalizedGlobals))
  }
  if (Array.isArray(config.plots)) {
    nextConfig.plots = []
    for (const plot of config.plots) nextConfig.plots.push(buildEffectivePlotData(plot, normalizedGlobals))
  }
  return nextConfig
}

export function applyGlobalDefaults(config, globalDefaults) {
  return createEditorEffectiveConfig({ config, globalDefaults })
}

export function syncGlobalDefaultsToConfig(config, globals, changedKeys = null) {
  if (!config || !globals) return config
  const changedKeySet = changedKeys ? new Set(changedKeys) : null
  const shouldApply = (key) => !changedKeySet || changedKeySet.has(key)
  const nextConfig = JSON.parse(JSON.stringify(config))
  if (nextConfig.labels) {
    for (const label of nextConfig.labels) {
      if (shouldApply('font_text')) Object.assign(label, createFontSelection(globals.font_text))
      if (shouldApply('color_text')) label.color = globals.color_text
    }
  }
  if (nextConfig.values) {
    for (const value of nextConfig.values) {
      if (shouldApply('font_values')) Object.assign(value, createFontSelection(globals.font_values))
      if (shouldApply('font_values') && value.display_variants?.linear) value.display_variants.linear.min_max_label_font = globals.font_values
      if (shouldApply('color_values')) value.color = globals.color_values
      if (shouldApply('color_icons') && Object.hasOwn(value, 'icon_color')) value.icon_color = globals.color_icons
      if (shouldApply('color_units') && value.value !== 'time') value.unit_color = globals.color_units
    }
  }
  if (nextConfig.plots) {
    for (const plot of nextConfig.plots) {
      if (plot.value === 'elevation' && shouldApply('font_values')) {
        plot.point_label = { ...(plot.point_label || {}), ...createFontSelection(globals.font_values) }
      }
      if (shouldApply('color_values') && Object.hasOwn(plot, 'color')) plot.color = globals.color_values
    }
  }
  return nextConfig
}
