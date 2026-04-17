import {
  Activity,
  Clock,
  Gauge,
  Map,
  Mountain,
  Thermometer,
  Timer,
  TrendingUp,
  Type,
  Zap,
} from 'lucide-react'

export const QUICKMENU_ITEMS = [
  { type: 'label', icon: Type, label: 'Text' },
  { type: 'speed', icon: Gauge, label: 'Speed' },
  { type: 'elevation', icon: Mountain, label: 'Elev.' },
  { type: 'heartrate', icon: Activity, label: 'HR' },
  { type: 'power', icon: Zap, label: 'Power' },
  { type: 'cadence', icon: Timer, label: 'Cadence' },
  { type: 'time', icon: Clock, label: 'Time' },
  { type: 'temperature', icon: Thermometer, label: 'Temp.' },
  { type: 'gradient', icon: TrendingUp, label: 'Grad.' },
  { type: 'course', icon: Map, label: 'Map' },
]

export const TYPE_LABELS = {
  label: 'Text',
  speed: 'Speed',
  elevation: 'Elevation',
  heartrate: 'Heart Rate',
  power: 'Power',
  cadence: 'Cadence',
  time: 'Time',
  temperature: 'Temperature',
  gradient: 'Gradient',
  course: 'Route Map',
}

export const TYPE_ICONS = {
  label: Type,
  speed: Gauge,
  elevation: Mountain,
  heartrate: Activity,
  power: Zap,
  cadence: Timer,
  time: Clock,
  temperature: Thermometer,
  gradient: TrendingUp,
  course: Map,
}

export function parseInteger(value, fallback = 0) {
  const parsed = parseInt(value, 10)
  return Number.isFinite(parsed) ? parsed : fallback
}

export function clamp(value, min, max) {
  return Math.min(max, Math.max(min, value))
}

export function getWidgetFont(widget, fallback = 'Arial.ttf') {
  return widget.data.font || widget.data.font_family || fallback
}

export function getGlobalColor(globalDefaults, key, fallback) {
  return globalDefaults?.[key] || fallback
}

export function createLabelDefaults(globalDefaults) {
  const font = globalDefaults?.font_text || 'Arial.ttf'
  return {
    x: 100,
    y: 100,
    font,
    font_family: font,
    font_size: 60,
    text: 'New Text',
    color: getGlobalColor(globalDefaults, 'color_text', '#ffffff'),
    opacity: globalDefaults?.opacity ?? 1,
  }
}

export function createMetricValueDefaults(type, globalDefaults) {
  const font = globalDefaults?.font_values || 'Furore.otf'
  return {
    x: 100,
    y: 100,
    value: type,
    font,
    font_family: font,
    font_size: type === 'time' ? 72 : type === 'gradient' ? 96 : 100,
    color: getGlobalColor(globalDefaults, 'color_values', '#ffffff'),
    opacity: globalDefaults?.opacity ?? 1,
    prefix: '',
    suffix: '',
    decimals: 0,
    show_icon: type !== 'gradient',
    icon_color: getGlobalColor(globalDefaults, 'color_icons', '#ffffff'),
    icon_size: 28,
    icon_offset_x: 0,
    icon_offset_y: 0,
    show_units: ['speed', 'temperature'].includes(type),
    speed_unit: 'kmh',
    temperature_unit: 'celsius',
    format: 'time-24',
    value_offset: 0,
    triangle_positive_color: '#22c55e',
    triangle_negative_color: '#ef4444',
    show_sign: true,
    show_triangle: true,
    triangle_width: 24,
    unit: 'metric',
  }
}

export function createPlotDefaults(type, globalDefaults) {
  const base = {
    x: 100,
    y: 100,
    value: type,
    width: 400,
    height: 200,
    opacity: globalDefaults?.opacity ?? 1,
    rotation: 0,
    completed_line_width: 6,
    remaining_line_width: 6,
  }

  if (type === 'course') {
    return {
      ...base,
      color: getGlobalColor(globalDefaults, 'color_values', '#ffffff'),
      completed_line_color: '#ffffff',
      completed_line_opacity: 100,
      remaining_line_color: '#71717a',
      remaining_line_opacity: 35,
      marker_size: 18,
      marker_color: '#ffffff',
      marker_opacity: 100,
    }
  }

  return {
    ...base,
    color: getGlobalColor(globalDefaults, 'color_values', '#ffffff'),
    completed_line_color: '#ffffff',
    completed_line_opacity: 100,
    remaining_line_color: '#71717a',
    remaining_line_opacity: 35,
    marker_size: 16,
    marker_color: '#ffffff',
    marker_opacity: 100,
    show_elevation_metric: true,
    show_elevation_imperial: false,
    metric_label_offset_x: 0,
    metric_label_offset_y: 0,
    imperial_label_offset_x: 0,
    imperial_label_offset_y: 0,
  }
}
