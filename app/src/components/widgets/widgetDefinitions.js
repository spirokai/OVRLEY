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
import { getThemeColor } from '@/lib/theme'
import { createFontSelection } from '@/lib/fonts'

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

function getCourseWidgetDimensions(coursePoints) {
  const validPoints = (coursePoints || []).filter(
    ([latitude, longitude]) =>
      Number.isFinite(latitude) && Number.isFinite(longitude),
  )

  if (validPoints.length < 2) {
    return { width: 400, height: 200 }
  }

  const meanLatitudeRadians =
    (validPoints.reduce((sum, [latitude]) => sum + latitude, 0) /
      validPoints.length) *
    (Math.PI / 180)
  const projectedX = validPoints.map(
    ([, longitude]) => longitude * Math.cos(meanLatitudeRadians),
  )
  const projectedY = validPoints.map(([latitude]) => latitude)
  const spanX = Math.max(
    Math.max(...projectedX) - Math.min(...projectedX),
    1e-6,
  )
  const spanY = Math.max(
    Math.max(...projectedY) - Math.min(...projectedY),
    1e-6,
  )

  if (spanX >= spanY) {
    return {
      width: 400,
      height: Math.max(Math.round((400 * spanY) / spanX), 80),
    }
  }

  return {
    width: Math.max(Math.round((400 * spanX) / spanY), 80),
    height: 400,
  }
}

export function createLabelDefaults(globalDefaults) {
  const font = globalDefaults?.font_text || 'Arial.ttf'
  const fontSelection = createFontSelection(font)
  return {
    x: 100,
    y: 100,
    ...fontSelection,
    font_size: 60,
    text: 'New Text',
    color: getGlobalColor(globalDefaults, 'color_text', getThemeColor('ice')),
    opacity: globalDefaults?.opacity ?? 1,
  }
}

export function createMetricValueDefaults(type, globalDefaults) {
  const font = globalDefaults?.font_values || 'Furore.otf'
  const fontSelection = createFontSelection(font)
  return {
    x: 100,
    y: 100,
    value: type,
    ...fontSelection,
    font_size: type === 'time' ? 72 : type === 'gradient' ? 96 : 100,
    color: getGlobalColor(globalDefaults, 'color_values', getThemeColor('ice')),
    opacity: globalDefaults?.opacity ?? 1,
    prefix: '',
    suffix: '',
    decimals: 0,
    show_icon: type !== 'gradient',
    icon_color: getGlobalColor(
      globalDefaults,
      'color_icons',
      getThemeColor('aqua'),
    ),
    icon_size: 28,
    icon_offset_x: 0,
    icon_offset_y: 0,
    show_units: ['speed', 'temperature'].includes(type),
    speed_unit: 'kmh',
    temperature_unit: 'celsius',
    format: 'time-24',
    value_offset: 0,
    triangle_positive_color: getThemeColor('aqua'),
    triangle_negative_color: getThemeColor('accent'),
    show_sign: true,
    show_triangle: true,
    triangle_width: 72,
    unit: 'metric',
  }
}

export function createPlotDefaults(type, globalDefaults, options = {}) {
  const courseDimensions =
    type === 'course'
      ? getCourseWidgetDimensions(options.coursePoints)
      : { width: 400, height: 200 }
  const base = {
    x: 100,
    y: 100,
    value: type,
    width: courseDimensions.width,
    height: courseDimensions.height,
    opacity: globalDefaults?.opacity ?? 1,
    rotation: 0,
    completed_line_width: 6,
    remaining_line_width: 6,
  }

  if (type === 'course') {
    return {
      ...base,
      color: getGlobalColor(
        globalDefaults,
        'color_values',
        getThemeColor('ice'),
      ),
      completed_line_color: getThemeColor('ice'),
      completed_line_opacity: 100,
      remaining_line_color: getThemeColor('teal'),
      remaining_line_opacity: 35,
      simplify_tolerance_px: 1,
      target_density: 1,
      marker_size: 18,
      marker_color: getThemeColor('aqua'),
      marker_opacity: 100,
    }
  }

  return {
    ...base,
    color: getGlobalColor(globalDefaults, 'color_values', getThemeColor('ice')),
    completed_line_color: getThemeColor('ice'),
    completed_line_opacity: 100,
    remaining_line_color: getThemeColor('teal'),
    remaining_line_opacity: 35,
    area_completed_color: getThemeColor('ice'),
    area_completed_opacity: 24,
    area_remaining_color: getThemeColor('teal'),
    area_remaining_opacity: 12,
    marker_size: 16,
    marker_color: getThemeColor('aqua'),
    marker_opacity: 100,
    show_elevation_metric: true,
    show_elevation_imperial: false,
    y_scale: 1,
    simplify_tolerance_px: 1,
    target_density: 0.75,
    metric_label_offset_x: 0,
    metric_label_offset_y: 0,
    imperial_label_offset_x: 0,
    imperial_label_offset_y: 0,
  }
}
