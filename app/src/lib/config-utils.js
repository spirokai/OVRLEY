/**
 * Provides shared config utils utilities for the app.
 */

import { createFontSelection, getFontFamilyName } from './fonts'

/**
 * Applies global defaults.
 *
 * @param {*} config - Overlay template configuration data.
 * @param {*} globals - Global defaults merged into widgets.
 * @returns {*} Result produced by the helper.
 */
export function applyGlobalDefaults(config, globals) {
  if (!config || !globals) return config

  // Deep clone to avoid mutating the original config in the store
  const newConfig = JSON.parse(JSON.stringify(config))

  // Apply to labels
  if (newConfig.labels) {
    newConfig.labels.forEach((label) => {
      if (!label.font) label.font = globals.font_text
      if (!label.font_family)
        label.font_family = getFontFamilyName(label.font || globals.font_text)
      if (!label.color) label.color = globals.color_text
      if (label.opacity === undefined) label.opacity = globals.opacity

      // Shadow & Border fallbacks
      if (label.shadow_color === undefined)
        label.shadow_color = globals.shadow_color
      if (label.shadow_strength === undefined)
        label.shadow_strength = globals.shadow_strength
      if (label.shadow_distance === undefined)
        label.shadow_distance = globals.shadow_distance
      if (label.border_color === undefined)
        label.border_color = globals.border_color
      if (label.border_thickness === undefined)
        label.border_thickness = globals.border_thickness
      if (label.border_strength === undefined)
        label.border_strength = globals.border_strength
      if (label.border_distance === undefined)
        label.border_distance = globals.border_distance
    })
  }

  // Apply to values
  if (newConfig.values) {
    newConfig.values.forEach((value) => {
      if (!value.font) value.font = globals.font_values
      if (!value.font_family)
        value.font_family = getFontFamilyName(value.font || globals.font_values)
      if (!value.color) value.color = globals.color_values
      if (value.icon_color === undefined) value.icon_color = globals.color_icons
      if (value.opacity === undefined) value.opacity = globals.opacity

      // Shadow & Border fallbacks
      if (value.shadow_color === undefined)
        value.shadow_color = globals.shadow_color
      if (value.shadow_strength === undefined)
        value.shadow_strength = globals.shadow_strength
      if (value.shadow_distance === undefined)
        value.shadow_distance = globals.shadow_distance
      if (value.border_color === undefined)
        value.border_color = globals.border_color
      if (value.border_thickness === undefined)
        value.border_thickness = globals.border_thickness
      if (value.border_strength === undefined)
        value.border_strength = globals.border_strength
      if (value.border_distance === undefined)
        value.border_distance = globals.border_distance
    })
  }

  // Apply to plots (charts/maps)
  if (newConfig.plots) {
    newConfig.plots.forEach((plot) => {
      if (!plot.color) plot.color = globals.color_values
      if (plot.opacity === undefined) plot.opacity = globals.opacity

      // Plots usually only have border or shadow if they are containers,
      // but we apply it for consistency
      if (plot.shadow_color === undefined)
        plot.shadow_color = globals.shadow_color
      if (plot.shadow_strength === undefined)
        plot.shadow_strength = globals.shadow_strength
    })
  }

  // Global Scale (applied to the scene)
  if (newConfig.scene) {
    newConfig.scene.scale = globals.scale
  }

  return newConfig
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

  if (nextConfig.labels) {
    nextConfig.labels.forEach((label) => {
      if (shouldApply('font_text')) {
        Object.assign(label, createFontSelection(globals.font_text))
      }
      if (shouldApply('color_text')) {
        label.color = globals.color_text
      }
      if (shouldApply('border_color')) {
        label.border_color = globals.border_color
      }
      if (shouldApply('border_thickness')) {
        label.border_thickness = globals.border_thickness
      }
      if (shouldApply('border_strength')) {
        label.border_strength = globals.border_strength
      }
      if (shouldApply('border_distance')) {
        label.border_distance = globals.border_distance
      }
      if (shouldApply('shadow_color')) {
        label.shadow_color = globals.shadow_color
      }
      if (shouldApply('shadow_strength')) {
        label.shadow_strength = globals.shadow_strength
      }
      if (shouldApply('shadow_distance')) {
        label.shadow_distance = globals.shadow_distance
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
      if (shouldApply('border_color')) {
        value.border_color = globals.border_color
      }
      if (shouldApply('border_thickness')) {
        value.border_thickness = globals.border_thickness
      }
      if (shouldApply('border_strength')) {
        value.border_strength = globals.border_strength
      }
      if (shouldApply('border_distance')) {
        value.border_distance = globals.border_distance
      }
      if (shouldApply('shadow_color')) {
        value.shadow_color = globals.shadow_color
      }
      if (shouldApply('shadow_strength')) {
        value.shadow_strength = globals.shadow_strength
      }
      if (shouldApply('shadow_distance')) {
        value.shadow_distance = globals.shadow_distance
      }
    })
  }

  if (nextConfig.plots) {
    nextConfig.plots.forEach((plot) => {
      if (shouldApply('color_values') && Object.hasOwn(plot, 'color')) {
        plot.color = globals.color_values
      }
      if (shouldApply('shadow_color')) {
        plot.shadow_color = globals.shadow_color
      }
      if (shouldApply('shadow_strength')) {
        plot.shadow_strength = globals.shadow_strength
      }
      if (shouldApply('shadow_distance')) {
        plot.shadow_distance = globals.shadow_distance
      }
    })
  }

  if (nextConfig.scene && shouldApply('scale')) {
    nextConfig.scene.scale = globals.scale
  }

  return nextConfig
}
