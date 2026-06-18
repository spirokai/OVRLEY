/**
 * Overlay editor configuration — font family map, widget icon lookup,
 * and default preview values.
 *
 * Contains only static data (constants, lookup tables, config objects).
 * No function definitions, no side effects, no React imports beyond
 * component references used as icon lookup values.
 */

import { WIDGET_ICONS } from '@/lib/widget/widget-icons'

/**
 * Maps font file names and short names to CSS font-family strings.
 * @type {Object<string, string>}
 */
export const FONT_FAMILY_MAP = {
  'Arial.ttf': 'Arial, Helvetica, sans-serif',
  Arial: 'Arial, Helvetica, sans-serif',
  'Evogria.otf': '"Evogria", Impact, Haettenschweiler, "Arial Narrow Bold", sans-serif',
  Evogria: '"Evogria", Impact, Haettenschweiler, "Arial Narrow Bold", sans-serif',
  'Furore.otf': '"Furore", "Arial Black", Impact, sans-serif',
  Furore: '"Furore", "Arial Black", Impact, sans-serif',
  'Saira Stencil.ttf': '"Saira Stencil", Impact, Haettenschweiler, "Arial Narrow Bold", sans-serif',
  'Saira Stencil': '"Saira Stencil", Impact, Haettenschweiler, "Arial Narrow Bold", sans-serif',
  'Teko.ttf': '"Teko", "Arial Narrow", sans-serif',
  Teko: '"Teko", "Arial Narrow", sans-serif',
}

export { WIDGET_ICONS }

/**
 * Default activity metric values used as fallback when no real activity is loaded.
 * @type {Object<string, number|string>}
 */
export const DEFAULT_ACTIVITY_PREVIEW = {
  altitude: 100,
  aperture: 2.8,
  cadence: 92,
  color_temperature: 5500,
  ev: 0,
  focal_length: 24,
  gradient: -7,
  heading: 90,
  heartrate: 154,
  iso: 400,
  power: 286,
  shutter_speed: 0.001,
  speed: 8.4,
  temperature: 21,
  time: '2026-04-20T09:41:00Z',
}
