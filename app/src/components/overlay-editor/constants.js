/**
 * Provides overlay editor helpers for constants.
 */

import {
  Clock3,
  Gauge,
  Heart,
  Map,
  Mountain,
  RefreshCw,
  Thermometer,
  TrendingUp,
  Type,
  Zap,
} from 'lucide-react'

export const FONT_FAMILY_MAP = {
  'Arial.ttf': 'Arial, Helvetica, sans-serif',
  Arial: 'Arial, Helvetica, sans-serif',
  'Evogria.otf':
    '"Evogria", Impact, Haettenschweiler, "Arial Narrow Bold", sans-serif',
  Evogria:
    '"Evogria", Impact, Haettenschweiler, "Arial Narrow Bold", sans-serif',
  'Furore.otf': '"Furore", "Arial Black", Impact, sans-serif',
  Furore: '"Furore", "Arial Black", Impact, sans-serif',
}

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

export const DEFAULT_ACTIVITY_PREVIEW = {
  cadence: 92,
  gradient: -7,
  heartrate: 154,
  power: 286,
  speed: 8.4,
  temperature: 21,
  time: '2026-04-20T09:41:00Z',
}

export const DEFAULT_GRADIENT_TRIANGLE_WIDTH = 72
