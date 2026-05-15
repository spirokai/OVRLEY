/**
 * Overlay editor configuration — font family map, widget icon lookup,
 * and default preview values.
 *
 * Contains only static data (constants, lookup tables, config objects).
 * No function definitions, no side effects, no React imports beyond
 * component references used as icon lookup values.
 */

import { Clock3, Gauge, Heart, Map, Mountain, RefreshCw, Thermometer, TrendingUp, Type, Zap } from 'lucide-react'

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
}

/**
 * Maps widget type names to Lucide icon components for badge rendering.
 * @type {Object<string, import('lucide-react').LucideIcon>}
 */
export const WIDGET_ICONS = {
  label: Type,
  speed: Gauge,
  heartrate: Heart,
  cadence: RefreshCw,
  power: Zap,
  time: Clock3,
  temperature: Thermometer,
  gradient: TrendingUp,
  course: Map,
  elevation: Mountain,
}

/**
 * Default activity metric values used as fallback when no real activity is loaded.
 * @type {Object<string, number|string>}
 */
export const DEFAULT_ACTIVITY_PREVIEW = {
  cadence: 92,
  gradient: -7,
  heartrate: 154,
  power: 286,
  speed: 8.4,
  temperature: 21,
  time: '2026-04-20T09:41:00Z',
}
