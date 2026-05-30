/**
 * @file template-defaults – Static constants and default values for the
 * template-state system.
 *
 * Owns all configuration constants that were previously embedded in
 * template-state.js. This module has zero runtime logic — it is purely
 * a data module. The orchestration layer (template-state.js) re-exports
 * these symbols so callers have a single import path.
 *
 * Sibling modules:
 * - template-normalization.js   owns normalization/cleanup of config shapes
 * - template-state.js           owns durable ↔ editor-effective materialization
 *
 * @module template-defaults
 */

export const SCENE_STYLE_DEFAULTS = {
  border_color: '#000000',
  border_thickness: 0,
  shadow_color: '#000000',
  shadow_strength: 0,
  shadow_distance: 0,
}

export const SCENE_STYLE_KEYS = Object.keys(SCENE_STYLE_DEFAULTS)

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
