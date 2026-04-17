/**
 * Merges global defaults into a configuration object.
 * This allows the user to set a "theme" in the Global tab and have it apply to all elements
 * that don't have explicit overrides.
 */
export function applyGlobalDefaults(config, globals) {
  if (!config || !globals) return config

  // Deep clone to avoid mutating the original config in the store
  const newConfig = JSON.parse(JSON.stringify(config))

  // Apply to labels
  if (newConfig.labels) {
    newConfig.labels.forEach((label) => {
      if (!label.font) label.font = globals.font_text
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
      if (!value.color) value.color = globals.color_values
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
