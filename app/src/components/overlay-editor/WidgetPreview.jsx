import { DEFAULT_GRADIENT_TRIANGLE_WIDTH, WIDGET_ICONS } from './constants'
import {
  areaToSvg,
  buildGradientTrianglePath,
  formatGradientValue,
  formatSpeed,
  formatTemperature,
  formatTimeValue,
  getCombinedTextShadow,
  getCompletedIndex,
  getPreviewFontFamily,
  getSampleValue,
  getWidgetOpacity,
  normalizeElevationPoints,
  normalizeRoutePoints,
  pointsToSvg,
} from './utils'

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

export default function WidgetPreview({
  widget,
  activity,
  sampleIndex,
  globalOpacity,
}) {
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
