import { useEffect, useMemo, useRef, useState } from 'react'
import Moveable from 'react-moveable'
import {
  Activity,
  Clock3,
  Gauge,
  LayoutGrid,
  Map,
  Mountain,
  Thermometer,
  Timer,
  TrendingUp,
  Type,
  Zap,
} from 'lucide-react'
import { cn } from '@/lib/utils'
import useStore from '../store/useStore'
import { getCurrentParsedActivity } from '../api/activityCache'
import { buildConfigWidgets, updateWidgetInConfig } from '@/lib/widget-config'
import { applyGlobalDefaults } from '@/lib/config-utils'

const FONT_FAMILY_MAP = {
  'Arial.ttf': 'Arial, Helvetica, sans-serif',
  'Evogria.otf': 'Impact, Haettenschweiler, "Arial Narrow Bold", sans-serif',
  'Furore.otf': '"Arial Black", Impact, sans-serif',
}

const WIDGET_ICONS = {
  label: Type,
  speed: Gauge,
  heartrate: Activity,
  cadence: Timer,
  power: Zap,
  time: Clock3,
  temperature: Thermometer,
  gradient: TrendingUp,
  course: Map,
  elevation: Mountain,
}

const DEFAULT_ACTIVITY_PREVIEW = {
  cadence: 92,
  gradient: -7,
  heartrate: 154,
  power: 286,
  speed: 8.4,
  temperature: 21,
  time: '2026-04-20T09:41:00Z',
}

const DEFAULT_GRADIENT_TRIANGLE_WIDTH = 72

function clamp(value, min, max) {
  return Math.min(max, Math.max(min, value))
}

function getSceneSize(config) {
  return {
    width: config?.scene?.width || 1920,
    height: config?.scene?.height || 1080,
  }
}

function getPreviewFontFamily(fontName) {
  return FONT_FAMILY_MAP[fontName] || FONT_FAMILY_MAP['Arial.ttf']
}

function getWidgetOpacity(data, globalOpacity = 1) {
  return clamp((data?.opacity ?? 1) * globalOpacity, 0, 1)
}

function getTextShadow(data) {
  const shadowStrength = Number(data?.shadow_strength) || 0
  const shadowDistance = Number(data?.shadow_distance) || 0
  const shadowColor = data?.shadow_color

  if (!shadowStrength || !shadowColor) return undefined

  return `${shadowDistance}px ${shadowDistance}px ${shadowStrength}px ${shadowColor}`
}

function getTextOutlineShadow(data) {
  const borderThickness = Number(data?.border_thickness) || 0
  const borderColor = data?.border_color

  if (!borderThickness || !borderColor) return ''

  const offsets = [
    [-1, 0],
    [1, 0],
    [0, -1],
    [0, 1],
    [-1, -1],
    [1, -1],
    [-1, 1],
    [1, 1],
  ]
  const layers = []

  for (let step = 1; step <= borderThickness; step += 1) {
    offsets.forEach(([x, y]) => {
      layers.push(`${x * step}px ${y * step}px 0 ${borderColor}`)
    })
  }

  return layers.join(', ')
}

function getCombinedTextShadow(data) {
  const outlineShadow = getTextOutlineShadow(data)
  const dropShadow = getTextShadow(data)

  if (outlineShadow && dropShadow) {
    return `${outlineShadow}, ${dropShadow}`
  }

  return outlineShadow || dropShadow || undefined
}

function findClosestSampleIndex(activity, selectedSecond) {
  const elapsedSeries = activity?.sample_elapsed_seconds || []
  if (!elapsedSeries.length) return 0

  let low = 0
  let high = elapsedSeries.length - 1
  let result = 0

  while (low <= high) {
    const middle = Math.floor((low + high) / 2)
    const candidate = Number(elapsedSeries[middle]) || 0

    if (candidate <= selectedSecond) {
      result = middle
      low = middle + 1
    } else {
      high = middle - 1
    }
  }

  return result
}

function getSampleValue(activity, key, sampleIndex) {
  const series = activity?.[key]
  if (!Array.isArray(series)) {
    return DEFAULT_ACTIVITY_PREVIEW[key] ?? null
  }

  return series[sampleIndex] ?? DEFAULT_ACTIVITY_PREVIEW[key] ?? null
}

function formatSpeed(value, unit) {
  const conversions = {
    kmh: { units: 'KM/H', factor: 3.6 },
    mph: { units: 'MPH', factor: 2.236936 },
    kn: { units: 'KN', factor: 1.943844 },
    mps: { units: 'M/S', factor: 1 },
  }
  const selection = conversions[unit] || conversions.kmh

  if (value === null || value === undefined) {
    return { value: '--', units: selection.units }
  }

  const numericValue = Number(value)
  return {
    value: Math.round(numericValue * selection.factor).toString(),
    units: selection.units,
  }
}

function formatTemperature(value, unit) {
  if (value === null || value === undefined) {
    return {
      value: '--',
      units: unit === 'fahrenheit' ? 'F' : 'C',
    }
  }

  const numericValue = Number(value)
  if (unit === 'fahrenheit') {
    return {
      value: Math.round((numericValue * 9) / 5 + 32).toString(),
      units: 'F',
    }
  }

  return {
    value: Math.round(numericValue).toString(),
    units: 'C',
  }
}

function padNumber(value) {
  return String(value).padStart(2, '0')
}

function formatTimeValue(format, timestamp) {
  if (!timestamp) return '--:--'

  const date = new Date(timestamp)
  if (!Number.isFinite(date.getTime())) return '--:--'

  const day = padNumber(date.getDate())
  const month = padNumber(date.getMonth() + 1)
  const year = date.getFullYear()
  const shortMonth = date
    .toLocaleString('en-US', { month: 'short' })
    .toUpperCase()
  const longMonth = date
    .toLocaleString('en-US', { month: 'long' })
    .toUpperCase()
  const hour24 = padNumber(date.getHours())
  const hour12Raw = date.getHours() % 12 || 12
  const hour12 = padNumber(hour12Raw)
  const minutes = padNumber(date.getMinutes())
  const suffix = date.getHours() >= 12 ? 'PM' : 'AM'

  const formatMap = {
    'date-dd-mm-yyyy': `${day}-${month}-${year}`,
    'date-mm-dd-yyyy': `${month}-${day}-${year}`,
    'date-yyyy-mm-dd': `${year}-${month}-${day}`,
    'date-dd-mmm-yyyy': `${day} ${shortMonth} ${year}`,
    'date-mmm-dd-yyyy': `${shortMonth} ${day} ${year}`,
    'date-dd-mmmm-yyyy': `${day} ${longMonth} ${year}`,
    'date-mmmm-dd-yyyy': `${longMonth} ${day} ${year}`,
    'time-24': `${hour24}:${minutes}`,
    'time-12': `${hour12}:${minutes} ${suffix}`,
    'date-time-24': `${day}-${month}-${year} ${hour24}:${minutes}`,
    'date-time-12': `${day}-${month}-${year} ${hour12}:${minutes} ${suffix}`,
    'date-mmm-time-24': `${day} ${shortMonth} ${hour24}:${minutes}`,
    'date-mmm-time-12': `${day} ${shortMonth} ${hour12}:${minutes} ${suffix}`,
    'date-mmmm-time-24': `${day} ${longMonth} ${hour24}:${minutes}`,
    'date-mmmm-time-12': `${day} ${longMonth} ${hour12}:${minutes} ${suffix}`,
  }

  return formatMap[format] || formatMap['time-24']
}

function formatGradientValue(widget, value) {
  if (value === null || value === undefined) return '--'

  const decimals = widget.data.decimals ?? 0
  const numericValue = Number(value)
  const absoluteValue = Math.abs(numericValue).toFixed(decimals)
  const sign = numericValue > 0 ? '+' : numericValue < 0 ? '-' : ''
  const prefix = widget.data.show_sign === false ? '' : sign

  return `${prefix}${absoluteValue}`
}

function buildGradientTrianglePath(value, width, height) {
  const normalized = clamp(Math.abs(Number(value) || 0) / 15, 0.12, 1)
  const rise = Math.max(height * normalized, 2)

  if (Number(value) >= 0) {
    return `M 0 ${height} L ${width} ${height} L ${width} ${height - rise} Z`
  }

  return `M 0 0 L ${width} 0 L ${width} ${rise} Z`
}

function buildFallbackRoute(width, height) {
  return [
    [width * 0.12, height * 0.82],
    [width * 0.3, height * 0.64],
    [width * 0.46, height * 0.72],
    [width * 0.64, height * 0.3],
    [width * 0.84, height * 0.18],
  ]
}

function normalizeRoutePoints(points, width, height, padding = 18) {
  const validPoints = points.filter(
    ([latitude, longitude]) =>
      Number.isFinite(latitude) && Number.isFinite(longitude),
  )

  if (validPoints.length < 2) {
    return buildFallbackRoute(width, height)
  }

  const latitudes = validPoints.map(([latitude]) => latitude)
  const longitudes = validPoints.map(([, longitude]) => longitude)
  const minLatitude = Math.min(...latitudes)
  const maxLatitude = Math.max(...latitudes)
  const minLongitude = Math.min(...longitudes)
  const maxLongitude = Math.max(...longitudes)
  const usableWidth = Math.max(width - padding * 2, 1)
  const usableHeight = Math.max(height - padding * 2, 1)
  const longitudeRange = Math.max(maxLongitude - minLongitude, 0.000001)
  const latitudeRange = Math.max(maxLatitude - minLatitude, 0.000001)
  const scaleX = usableWidth / longitudeRange
  const scaleY = usableHeight / latitudeRange
  const offsetX = (width - usableWidth) / 2
  const offsetY = (height - usableHeight) / 2

  return validPoints.map(([latitude, longitude]) => {
    const x = offsetX + (longitude - minLongitude) * scaleX
    const y = height - (offsetY + (latitude - minLatitude) * scaleY)
    return [x, y]
  })
}

function buildWidgetTransform({ scale = 1, rotation = 0 }) {
  const transforms = []

  if (rotation) {
    transforms.push(`rotate(${rotation}deg)`)
  }

  if (scale !== 1) {
    transforms.push(`scale(${scale})`)
  }

  return transforms.length ? transforms.join(' ') : undefined
}

function normalizeElevationPoints(values, width, height, padding = 18) {
  const usableValues = values.filter((value) => Number.isFinite(value))
  if (!usableValues.length) {
    return [
      [padding, height - padding],
      [width * 0.32, height * 0.55],
      [width * 0.62, height * 0.36],
      [width - padding, height * 0.48],
    ]
  }

  const minimum = Math.min(...usableValues)
  const maximum = Math.max(...usableValues)
  const amplitude = Math.max(maximum - minimum, 1)
  const step =
    usableValues.length > 1
      ? (width - padding * 2) / (usableValues.length - 1)
      : 0

  return usableValues.map((value, index) => {
    const x = padding + index * step
    const y =
      height -
      padding -
      ((value - minimum) / amplitude) * (height - padding * 2)
    return [x, y]
  })
}

function pointsToSvg(points) {
  return points.map(([x, y]) => `${x},${y}`).join(' ')
}

function areaToSvg(points, width, height, padding = 18) {
  if (!points.length) return ''
  return [
    `${padding},${height - padding}`,
    ...points.map(([x, y]) => `${x},${y}`),
    `${width - padding},${height - padding}`,
  ].join(' ')
}

function getCompletedIndex(totalPoints, sampleIndex) {
  if (totalPoints <= 1) return 0
  return clamp(sampleIndex, 0, totalPoints - 1)
}

function OverlayMetricWidget({ widget, activity, sampleIndex, globalOpacity }) {
  const Icon = WIDGET_ICONS[widget.type]
  const fontSize = widget.data.font_size ?? 60
  const fontFamily = getPreviewFontFamily(
    widget.data.font || widget.data.font_family,
  )
  const color = widget.data.color || '#ffffff'
  const textStyle = {
    color,
    fontFamily,
    fontSize,
    lineHeight: 0.92,
    opacity: getWidgetOpacity(widget.data, globalOpacity),
    textShadow: getCombinedTextShadow(widget.data),
  }

  let valueText = '--'
  let unitText = ''

  if (widget.type === 'speed') {
    const speedUnit =
      widget.data.speed_unit ||
      (widget.data.unit === 'imperial' ? 'mph' : 'kmh')
    const formatted = formatSpeed(
      getSampleValue(activity, 'speed', sampleIndex),
      speedUnit,
    )
    valueText = formatted.value
    unitText = formatted.units
  } else if (widget.type === 'heartrate') {
    const value = getSampleValue(activity, 'heartrate', sampleIndex)
    valueText =
      value === null || value === undefined
        ? '--'
        : Math.round(value).toString()
    unitText = 'BPM'
  } else if (widget.type === 'cadence') {
    const value = getSampleValue(activity, 'cadence', sampleIndex)
    valueText =
      value === null || value === undefined
        ? '--'
        : Math.round(value).toString()
    unitText = 'RPM'
  } else if (widget.type === 'power') {
    const value = getSampleValue(activity, 'power', sampleIndex)
    valueText =
      value === null || value === undefined
        ? '--'
        : Math.round(value).toString()
    unitText = 'W'
  } else if (widget.type === 'time') {
    valueText = formatTimeValue(
      widget.data.format || 'time-24',
      getSampleValue(activity, 'time', sampleIndex),
    )
  } else if (widget.type === 'temperature') {
    const formatted = formatTemperature(
      getSampleValue(activity, 'temperature', sampleIndex),
      widget.data.temperature_unit || 'celsius',
    )
    valueText = formatted.value
    unitText = formatted.units
  } else if (widget.type === 'gradient') {
    valueText = `${formatGradientValue(widget, getSampleValue(activity, 'gradient', sampleIndex))}%`
  }

  const currentGradientValue = Number(
    getSampleValue(activity, 'gradient', sampleIndex) ?? 0,
  )
  const iconSize = widget.data.icon_size ?? 28
  const iconWrapperStyle = {
    marginRight: Math.max(fontSize * 0.08, 8),
    transform: `translate(${widget.data.icon_offset_x ?? 0}px, ${widget.data.icon_offset_y ?? 0}px)`,
    opacity: getWidgetOpacity(widget.data, globalOpacity),
  }
  const iconStyle = {
    color: widget.data.icon_color || '#40e0d0',
    width: iconSize,
    height: iconSize,
    display: 'block',
  }
  const showUnits =
    widget.data.show_units ?? ['speed', 'temperature'].includes(widget.type)
  const showIcon = widget.data.show_icon ?? widget.type !== 'gradient'
  const gradientValueOffset =
    widget.type === 'gradient' ? (widget.data.value_offset ?? 0) : 0

  return (
    <div className="inline-flex w-max flex-col items-center gap-2">
      <div
        className="inline-flex items-center gap-2 whitespace-nowrap"
        style={{
          ...textStyle,
          ...(widget.type === 'gradient'
            ? {
                marginTop: Math.max(gradientValueOffset, 0),
                marginBottom: Math.max(-gradientValueOffset, 0),
              }
            : {
                transform: `translateY(${widget.data.value_offset ?? 0}px)`,
              }),
        }}
      >
        {showIcon && Icon ? (
          <span
            className="inline-flex shrink-0 self-center"
            style={iconWrapperStyle}
          >
            <Icon style={iconStyle} />
          </span>
        ) : null}
        <div className="inline-flex items-end gap-2">
          <span>{valueText}</span>
          {showUnits && unitText ? (
            <span
              style={{
                fontSize: Math.max(fontSize * 0.28, 12),
                opacity: 'inherit',
              }}
            >
              {unitText}
            </span>
          ) : null}
        </div>
      </div>
      {widget.type === 'gradient' && widget.data.show_triangle !== false ? (
        <div className="flex w-full justify-center">
          {(() => {
            const triangleWidth = Math.max(
              widget.data.triangle_width ?? DEFAULT_GRADIENT_TRIANGLE_WIDTH,
              8,
            )
            const triangleHeight = Math.max(triangleWidth * 0.42, 8)

            return (
              <svg
                width={triangleWidth}
                height={triangleHeight}
                viewBox={`0 0 ${triangleWidth} ${triangleHeight}`}
                style={{
                  opacity: getWidgetOpacity(widget.data, globalOpacity),
                }}
              >
                <path
                  d={buildGradientTrianglePath(
                    currentGradientValue,
                    triangleWidth,
                    triangleHeight,
                  )}
                  fill={
                    currentGradientValue >= 0
                      ? widget.data.triangle_positive_color || '#40e0d0'
                      : widget.data.triangle_negative_color || '#c65102'
                  }
                />
              </svg>
            )
          })()}
        </div>
      ) : null}
    </div>
  )
}

function OverlayTextWidget({ widget, globalOpacity }) {
  const fontSize = widget.data.font_size ?? 60
  return (
    <div
      className="whitespace-nowrap"
      style={{
        color: widget.data.color || '#ffffff',
        fontFamily: getPreviewFontFamily(
          widget.data.font || widget.data.font_family,
        ),
        fontSize,
        lineHeight: 0.92,
        opacity: getWidgetOpacity(widget.data, globalOpacity),
        textShadow: getCombinedTextShadow(widget.data),
      }}
    >
      {widget.data.text || 'TEXT'}
    </div>
  )
}

function OverlayRouteWidget({ widget, activity, sampleIndex, globalOpacity }) {
  const width = Math.max(widget.data.width ?? 320, 80)
  const height = Math.max(widget.data.height ?? 180, 80)
  const points = normalizeRoutePoints(
    activity?.sample_course_points || [],
    width,
    height,
  )
  const completedIndex = getCompletedIndex(points.length, sampleIndex)
  const completedPoints = points.slice(0, completedIndex + 1)
  const remainingPoints = points.slice(completedIndex)
  const markerPoint = points[completedIndex] || points[points.length - 1]

  return (
    <svg
      width={width}
      height={height}
      viewBox={`0 0 ${width} ${height}`}
      style={{ opacity: getWidgetOpacity(widget.data, globalOpacity) }}
    >
      <g>
        <polyline
          fill="none"
          stroke={widget.data.remaining_line_color || '#005b5b'}
          strokeOpacity={(widget.data.remaining_line_opacity ?? 35) / 100}
          strokeWidth={widget.data.remaining_line_width ?? 6}
          strokeLinejoin="round"
          strokeLinecap="round"
          points={pointsToSvg(
            remainingPoints.length > 1 ? remainingPoints : points,
          )}
        />
        <polyline
          fill="none"
          stroke={widget.data.completed_line_color || '#afeeee'}
          strokeOpacity={(widget.data.completed_line_opacity ?? 100) / 100}
          strokeWidth={widget.data.completed_line_width ?? 6}
          strokeLinejoin="round"
          strokeLinecap="round"
          points={pointsToSvg(
            completedPoints.length > 1 ? completedPoints : points.slice(0, 2),
          )}
        />
        {markerPoint ? (
          <circle
            cx={markerPoint[0]}
            cy={markerPoint[1]}
            r={widget.data.marker_size ?? 18}
            fill={widget.data.marker_color || '#40e0d0'}
            fillOpacity={(widget.data.marker_opacity ?? 100) / 100}
          />
        ) : null}
      </g>
    </svg>
  )
}

function OverlayElevationWidget({
  widget,
  activity,
  sampleIndex,
  globalOpacity,
}) {
  const width = Math.max(widget.data.width ?? 320, 80)
  const height = Math.max(widget.data.height ?? 180, 80)
  const points = normalizeElevationPoints(
    activity?.sample_elevations || [],
    width,
    height,
  )
  const completedIndex = getCompletedIndex(points.length, sampleIndex)
  const completedPoints = points.slice(0, completedIndex + 1)
  const remainingPoints = points.slice(completedIndex)
  const markerPoint = points[completedIndex] || points[points.length - 1]
  const elevationValue = getSampleValue(activity, 'elevation', sampleIndex)
  const metricLabel =
    elevationValue === null || elevationValue === undefined
      ? '-- m'
      : `${Math.round(elevationValue)} m`
  const imperialLabel =
    elevationValue === null || elevationValue === undefined
      ? '-- ft'
      : `${Math.round(elevationValue * 3.28084)} ft`

  return (
    <div
      className="relative"
      style={{
        width,
        height,
        opacity: getWidgetOpacity(widget.data, globalOpacity),
      }}
    >
      <svg width={width} height={height} viewBox={`0 0 ${width} ${height}`}>
        <polygon
          points={areaToSvg(points, width, height)}
          fill={widget.data.remaining_line_color || '#005b5b'}
          fillOpacity={0.12}
        />
        <polyline
          fill="none"
          stroke={widget.data.remaining_line_color || '#005b5b'}
          strokeOpacity={(widget.data.remaining_line_opacity ?? 35) / 100}
          strokeWidth={widget.data.remaining_line_width ?? 6}
          strokeLinejoin="round"
          strokeLinecap="round"
          points={pointsToSvg(
            remainingPoints.length > 1 ? remainingPoints : points,
          )}
        />
        <polyline
          fill="none"
          stroke={widget.data.completed_line_color || '#afeeee'}
          strokeOpacity={(widget.data.completed_line_opacity ?? 100) / 100}
          strokeWidth={widget.data.completed_line_width ?? 6}
          strokeLinejoin="round"
          strokeLinecap="round"
          points={pointsToSvg(
            completedPoints.length > 1 ? completedPoints : points.slice(0, 2),
          )}
        />
        {markerPoint ? (
          <circle
            cx={markerPoint[0]}
            cy={markerPoint[1]}
            r={widget.data.marker_size ?? 16}
            fill={widget.data.marker_color || '#40e0d0'}
            fillOpacity={(widget.data.marker_opacity ?? 100) / 100}
          />
        ) : null}
      </svg>
      {markerPoint && widget.data.show_elevation_metric !== false ? (
        <div
          className="absolute text-[11px] font-semibold"
          style={{
            left: markerPoint[0] + (widget.data.metric_label_offset_x ?? 0),
            top: markerPoint[1] + (widget.data.metric_label_offset_y ?? 0) - 28,
            color: widget.data.color || '#afeeee',
          }}
        >
          {metricLabel}
        </div>
      ) : null}
      {markerPoint && widget.data.show_elevation_imperial ? (
        <div
          className="absolute text-[11px] font-semibold"
          style={{
            left: markerPoint[0] + (widget.data.imperial_label_offset_x ?? 0),
            top:
              markerPoint[1] + (widget.data.imperial_label_offset_y ?? 0) + 6,
            color: widget.data.color || '#afeeee',
          }}
        >
          {imperialLabel}
        </div>
      ) : null}
    </div>
  )
}

function WidgetPreview({ widget, activity, sampleIndex, globalOpacity }) {
  if (widget.type === 'label') {
    return <OverlayTextWidget widget={widget} globalOpacity={globalOpacity} />
  }

  if (widget.type === 'course') {
    return (
      <OverlayRouteWidget
        widget={widget}
        activity={activity}
        sampleIndex={sampleIndex}
        globalOpacity={globalOpacity}
      />
    )
  }

  if (widget.type === 'elevation') {
    return (
      <OverlayElevationWidget
        widget={widget}
        activity={activity}
        sampleIndex={sampleIndex}
        globalOpacity={globalOpacity}
      />
    )
  }

  return (
    <OverlayMetricWidget
      widget={widget}
      activity={activity}
      sampleIndex={sampleIndex}
      globalOpacity={globalOpacity}
    />
  )
}

function EmptyOverlayState() {
  return (
    <div className="flex h-full items-center justify-center p-8">
      <div className="max-w-sm rounded-xl border border-dashed border-border/70 bg-card/60 px-8 py-10 text-center shadow-[0_30px_80px_rgba(0,0,0,0.25)] backdrop-blur-sm">
        <div className="mx-auto mb-4 flex h-14 w-14 items-center justify-center bg-surface-elevated text-primary">
          <LayoutGrid className="h-6 w-6" />
        </div>
        <p className="text-sm font-semibold text-foreground">
          Overlay canvas ready
        </p>
        <p className="mt-2 text-sm text-muted-foreground">
          Load a template or add widgets to start positioning the overlay.
        </p>
      </div>
    </div>
  )
}

export default function OverlayEditor({
  config,
  globalDefaults,
  onConfigChange,
  zoomLevel,
  onZoomLevelChange,
  backgroundMode,
}) {
  const { selectedWidgetId, setSelectedWidgetId, selectedSecond } = useStore()
  const viewportRef = useRef(null)
  const moveableRef = useRef(null)
  const interactionStartRef = useRef(null)
  const draftWidgetsRef = useRef({})
  const [viewportSize, setViewportSize] = useState({ width: 0, height: 0 })
  const [sceneElement, setSceneElement] = useState(null)
  const [widgetNodes, setWidgetNodes] = useState({})
  const [draftWidgets, setDraftWidgets] = useState({})

  const activity = getCurrentParsedActivity()
  const resolvedConfig = useMemo(
    () =>
      applyGlobalDefaults(config, {
        ...globalDefaults,
        opacity: 1,
        scale: 1,
      }),
    [config, globalDefaults],
  )
  const widgets = useMemo(
    () => buildConfigWidgets(resolvedConfig),
    [resolvedConfig],
  )
  const sceneSize = useMemo(
    () => getSceneSize(resolvedConfig),
    [resolvedConfig],
  )
  const globalOpacity = globalDefaults?.opacity ?? 1
  const globalScale = globalDefaults?.scale ?? 1
  const sampleIndex = useMemo(
    () => findClosestSampleIndex(activity, selectedSecond),
    [activity, selectedSecond],
  )

  useEffect(() => {
    setDraftWidgets({})
  }, [resolvedConfig])

  useEffect(() => {
    draftWidgetsRef.current = draftWidgets
  }, [draftWidgets])

  useEffect(() => {
    const viewportNode = viewportRef.current
    if (!viewportNode || typeof ResizeObserver === 'undefined') return undefined

    const resizeObserver = new ResizeObserver(([entry]) => {
      const nextWidth = entry?.contentRect?.width || viewportNode.clientWidth
      const nextHeight = entry?.contentRect?.height || viewportNode.clientHeight
      setViewportSize({ width: nextWidth, height: nextHeight })
    })

    resizeObserver.observe(viewportNode)
    return () => resizeObserver.disconnect()
  }, [])

  const fitScale = useMemo(() => {
    const safeWidth = Math.max(viewportSize.width - 72, 1)
    const safeHeight = Math.max(viewportSize.height - 72, 1)
    return Math.min(
      safeWidth / sceneSize.width,
      safeHeight / sceneSize.height,
      1,
    )
  }, [viewportSize, sceneSize])

  const displayScale = fitScale * zoomLevel
  const selectedWidget = useMemo(
    () => widgets.find((widget) => widget.id === selectedWidgetId) || null,
    [widgets, selectedWidgetId],
  )
  const selectedWidgetDataSignature = useMemo(
    () => JSON.stringify(selectedWidget?.data ?? null),
    [selectedWidget],
  )
  const selectedTarget = selectedWidgetId
    ? widgetNodes[selectedWidgetId] || null
    : null
  const elementGuidelines = widgets
    .filter((widget) => widget.id !== selectedWidgetId)
    .map((widget) => widgetNodes[widget.id])
    .filter(Boolean)

  useEffect(() => {
    if (!moveableRef.current || !selectedTarget) return undefined

    const frameId = requestAnimationFrame(() => {
      moveableRef.current?.updateRect()
    })

    return () => cancelAnimationFrame(frameId)
  }, [
    selectedTarget,
    selectedWidgetId,
    selectedWidgetDataSignature,
    globalScale,
    displayScale,
  ])

  const canResizeSelected = selectedWidget?.category === 'plots'
  const canScaleSelected = Boolean(
    selectedWidget && selectedWidget.category !== 'plots',
  )
  const canRotateSelected = selectedWidget?.type === 'course'

  const widgetRefCallbacks = useMemo(
    () =>
      Object.fromEntries(
        widgets.map((widget) => [
          widget.id,
          (node) => {
            setWidgetNodes((current) => {
              if (node && current[widget.id] === node) return current
              if (!node && !current[widget.id]) return current

              const next = { ...current }
              if (node) {
                next[widget.id] = node
              } else {
                delete next[widget.id]
              }
              return next
            })
          },
        ]),
      ),
    [widgets],
  )

  const commitWidgetUpdate = (widgetId, updates) => {
    if (!resolvedConfig) return
    onConfigChange(updateWidgetInConfig(resolvedConfig, widgetId, updates))
  }

  const handleWheel = (event) => {
    event.preventDefault()
    const delta = event.deltaY < 0 ? 0.1 : -0.1
    onZoomLevelChange((current) =>
      clamp(Number((current + delta).toFixed(2)), 0.35, 4),
    )
  }

  if (!resolvedConfig) {
    return <EmptyOverlayState />
  }

  return (
    <div
      ref={viewportRef}
      className="relative flex h-full flex-1 overflow-hidden"
      onWheel={handleWheel}
    >
      <div className="flex h-full w-full items-center justify-center overflow-hidden p-8">
        <div
          className="relative shrink-0"
          style={{
            width: sceneSize.width * displayScale,
            height: sceneSize.height * displayScale,
          }}
        >
          <div
            className="absolute left-0 top-0"
            style={{
              width: sceneSize.width,
              height: sceneSize.height,
              transform: `scale(${displayScale})`,
              transformOrigin: 'top left',
            }}
          >
            <div
              ref={setSceneElement}
              className="relative overflow-visible"
              style={{
                width: sceneSize.width,
                height: sceneSize.height,
              }}
            >
              <div
                className={cn(
                  'absolute inset-0 overflow-hidden rounded-md shadow-[0_5px_20px_3px_rgba(0,0,0,0.2)]',
                  backgroundMode === 'checker' && 'bg-overlay-grid-muted',
                )}
                style={{ backgroundColor: '#000000' }}
              >
                <div className="absolute inset-0 " />
                {widgets.map((widget) => {
                  const draft = draftWidgets[widget.id]
                  const previewWidget = draft
                    ? {
                        ...widget,
                        data: {
                          ...widget.data,
                          ...draft,
                        },
                      }
                    : widget
                  const x = previewWidget.data.x ?? 0
                  const y = previewWidget.data.y ?? 0
                  const scale = (draft?.scale ?? 1) * globalScale
                  const rotation =
                    previewWidget.type === 'course'
                      ? (previewWidget.data.rotation ?? 0)
                      : 0
                  const width = previewWidget.data.width
                  const height = previewWidget.data.height
                  const Icon = WIDGET_ICONS[previewWidget.type] || Type

                  return (
                    <div
                      key={previewWidget.id}
                      ref={widgetRefCallbacks[previewWidget.id]}
                      className="group absolute cursor-move select-none rounded-xl border border-transparent transition-shadow"
                      style={{
                        left: x,
                        top: y,
                        width,
                        height,
                        transform: buildWidgetTransform({ scale, rotation }),
                        transformOrigin:
                          previewWidget.type === 'course'
                            ? 'center center'
                            : 'top left',
                      }}
                      onMouseDown={(event) => {
                        event.stopPropagation()
                        setSelectedWidgetId(previewWidget.id)
                      }}
                    >
                      <div className="absolute -top-7 left-0 flex items-center gap-1 rounded-full border border-border/70 bg-card/80 px-2 py-1 text-[10px] font-semibold text-muted-foreground opacity-0 transition-opacity group-hover:opacity-100">
                        <Icon className="h-3 w-3" />
                        <span>{previewWidget.type}</span>
                      </div>
                      <WidgetPreview
                        widget={previewWidget}
                        activity={activity}
                        sampleIndex={sampleIndex}
                        globalOpacity={globalOpacity}
                      />
                    </div>
                  )
                })}
              </div>
            </div>

            {selectedTarget && sceneElement ? (
              <Moveable
                ref={moveableRef}
                className="cyclemetry-moveable"
                target={selectedTarget}
                container={sceneElement}
                origin={false}
                edge={false}
                draggable
                resizable={canResizeSelected}
                scalable={canScaleSelected}
                rotatable={canRotateSelected}
                renderDirections={['nw', 'ne', 'sw', 'se']}
                snappable
                snapThreshold={8}
                snapGap
                keepRatio={false}
                elementGuidelines={elementGuidelines}
                horizontalGuidelines={[
                  0,
                  sceneSize.height / 2,
                  sceneSize.height,
                ]}
                verticalGuidelines={[0, sceneSize.width / 2, sceneSize.width]}
                bounds={{
                  left: 0,
                  top: 0,
                  right: sceneSize.width,
                  bottom: sceneSize.height,
                }}
                zoom={1}
                onDragStart={() => {
                  interactionStartRef.current = {
                    id: selectedWidget.id,
                    x: selectedWidget.data.x ?? 0,
                    y: selectedWidget.data.y ?? 0,
                  }
                }}
                onDrag={({ beforeTranslate }) => {
                  const origin = interactionStartRef.current
                  if (!origin?.id) return

                  setDraftWidgets((current) => ({
                    ...current,
                    [origin.id]: {
                      ...current[origin.id],
                      x: origin.x + beforeTranslate[0],
                      y: origin.y + beforeTranslate[1],
                    },
                  }))
                }}
                onDragEnd={() => {
                  const origin = interactionStartRef.current
                  if (!origin?.id) return

                  const draft = draftWidgetsRef.current[origin.id]
                  if (draft) {
                    commitWidgetUpdate(origin.id, {
                      x: Math.round(draft.x ?? origin.x),
                      y: Math.round(draft.y ?? origin.y),
                    })
                  }

                  setDraftWidgets((current) => {
                    const next = { ...current }
                    delete next[origin.id]
                    return next
                  })
                  interactionStartRef.current = null
                }}
                onResizeStart={({ dragStart }) => {
                  if (dragStart) {
                    dragStart.set([0, 0])
                  }

                  interactionStartRef.current = {
                    id: selectedWidget.id,
                    x: selectedWidget.data.x ?? 0,
                    y: selectedWidget.data.y ?? 0,
                    width: selectedWidget.data.width ?? 0,
                    height: selectedWidget.data.height ?? 0,
                    markerSize: selectedWidget.data.marker_size ?? null,
                  }
                }}
                onResize={({ width, height, drag }) => {
                  const origin = interactionStartRef.current
                  if (!origin?.id) return

                  const nextX = origin.x + drag.beforeTranslate[0]
                  const nextY = origin.y + drag.beforeTranslate[1]
                  const nextWidth = Math.max(width, 8)
                  const nextHeight = Math.max(height, 8)
                  const widthScale = origin.width ? nextWidth / origin.width : 1
                  const heightScale = origin.height
                    ? nextHeight / origin.height
                    : 1
                  const markerScale = (widthScale + heightScale) / 2
                  const nextMarkerSize =
                    origin.markerSize === null
                      ? undefined
                      : clamp(
                          Math.round(origin.markerSize * markerScale),
                          0,
                          400,
                        )

                  if (drag.target) {
                    drag.target.style.left = `${nextX}px`
                    drag.target.style.top = `${nextY}px`
                    drag.target.style.width = `${nextWidth}px`
                    drag.target.style.height = `${nextHeight}px`
                  }

                  setDraftWidgets((current) => ({
                    ...current,
                    [origin.id]: {
                      ...current[origin.id],
                      x: nextX,
                      y: nextY,
                      width: nextWidth,
                      height: nextHeight,
                      ...(nextMarkerSize === undefined
                        ? {}
                        : { marker_size: nextMarkerSize }),
                    },
                  }))
                }}
                onResizeEnd={() => {
                  const origin = interactionStartRef.current
                  if (!origin?.id) return

                  const draft = draftWidgetsRef.current[origin.id]
                  if (draft) {
                    commitWidgetUpdate(origin.id, {
                      x: Math.round(draft.x ?? origin.x),
                      y: Math.round(draft.y ?? origin.y),
                      width: Math.max(Math.round(draft.width ?? 0), 0),
                      height: Math.max(Math.round(draft.height ?? 0), 0),
                      ...(draft.marker_size === undefined
                        ? {}
                        : {
                            marker_size: Math.max(
                              Math.round(draft.marker_size),
                              0,
                            ),
                          }),
                    })
                  }

                  setDraftWidgets((current) => {
                    const next = { ...current }
                    delete next[origin.id]
                    return next
                  })
                  interactionStartRef.current = null
                }}
                onScaleStart={({ dragStart }) => {
                  if (dragStart) {
                    dragStart.set([0, 0])
                  }

                  interactionStartRef.current = {
                    id: selectedWidget.id,
                    x: selectedWidget.data.x ?? 0,
                    y: selectedWidget.data.y ?? 0,
                    fontSize: selectedWidget.data.font_size ?? 60,
                    iconSize: selectedWidget.data.icon_size ?? 28,
                    iconOffsetX: selectedWidget.data.icon_offset_x ?? 0,
                    iconOffsetY: selectedWidget.data.icon_offset_y ?? 0,
                    triangleWidth:
                      selectedWidget.data.triangle_width ??
                      DEFAULT_GRADIENT_TRIANGLE_WIDTH,
                    valueOffset: selectedWidget.data.value_offset ?? 0,
                  }
                }}
                onScale={({ scale, drag }) => {
                  const origin = interactionStartRef.current
                  if (!origin?.id) return

                  setDraftWidgets((current) => ({
                    ...current,
                    [origin.id]: {
                      ...current[origin.id],
                      x: origin.x + drag.beforeTranslate[0],
                      y: origin.y + drag.beforeTranslate[1],
                      scale: Math.max(scale[0], scale[1]),
                    },
                  }))
                }}
                onRotateStart={() => {
                  interactionStartRef.current = {
                    id: selectedWidget.id,
                    x: selectedWidget.data.x ?? 0,
                    y: selectedWidget.data.y ?? 0,
                    rotation: selectedWidget.data.rotation ?? 0,
                  }
                }}
                onRotate={({ beforeRotate, drag, target }) => {
                  const origin = interactionStartRef.current
                  if (!origin?.id) return

                  const nextX = origin.x + (drag?.beforeTranslate?.[0] ?? 0)
                  const nextY = origin.y + (drag?.beforeTranslate?.[1] ?? 0)
                  const nextRotation = beforeRotate
                  const currentDraft = draftWidgetsRef.current[origin.id] || {}
                  const scale = (currentDraft.scale ?? 1) * globalScale

                  if (target) {
                    target.style.left = `${nextX}px`
                    target.style.top = `${nextY}px`
                    target.style.transform =
                      buildWidgetTransform({
                        scale,
                        rotation: nextRotation,
                      }) || ''
                  }

                  setDraftWidgets((current) => ({
                    ...current,
                    [origin.id]: {
                      ...current[origin.id],
                      x: nextX,
                      y: nextY,
                      rotation: nextRotation,
                    },
                  }))
                }}
                onRotateEnd={() => {
                  const origin = interactionStartRef.current
                  if (!origin?.id) return

                  const draft = draftWidgetsRef.current[origin.id]
                  if (draft) {
                    const normalizedRotation =
                      (((draft.rotation ?? origin.rotation ?? 0) % 360) + 360) %
                      360

                    commitWidgetUpdate(origin.id, {
                      x: Math.round(draft.x ?? origin.x),
                      y: Math.round(draft.y ?? origin.y),
                      rotation: Number(normalizedRotation.toFixed(1)),
                    })
                  }

                  setDraftWidgets((current) => {
                    const next = { ...current }
                    delete next[origin.id]
                    return next
                  })
                  interactionStartRef.current = null
                }}
                onScaleEnd={() => {
                  const origin = interactionStartRef.current
                  if (!origin?.id) return

                  const draft = draftWidgetsRef.current[origin.id]
                  if (draft) {
                    const scaleFactor = draft.scale ?? 1
                    const nextFontSize = clamp(
                      Math.round((origin.fontSize || 60) * scaleFactor),
                      8,
                      400,
                    )
                    const nextIconSize = clamp(
                      Math.round((origin.iconSize || 28) * scaleFactor),
                      0,
                      400,
                    )
                    const nextIconOffsetX = Math.round(
                      (origin.iconOffsetX || 0) * scaleFactor,
                    )
                    const nextIconOffsetY = Math.round(
                      (origin.iconOffsetY || 0) * scaleFactor,
                    )
                    const nextTriangleWidth = clamp(
                      Math.round(origin.triangleWidth * scaleFactor),
                      0,
                      600,
                    )
                    const nextValueOffset = Math.round(
                      (origin.valueOffset || 0) * scaleFactor,
                    )

                    commitWidgetUpdate(origin.id, {
                      x: Math.round(draft.x ?? origin.x),
                      y: Math.round(draft.y ?? origin.y),
                      font_size: nextFontSize,
                      icon_size: nextIconSize,
                      icon_offset_x: nextIconOffsetX,
                      icon_offset_y: nextIconOffsetY,
                      triangle_width: nextTriangleWidth,
                      value_offset: nextValueOffset,
                    })
                  }

                  setDraftWidgets((current) => {
                    const next = { ...current }
                    delete next[origin.id]
                    return next
                  })
                  interactionStartRef.current = null
                }}
              />
            ) : null}
          </div>
        </div>
      </div>
    </div>
  )
}
