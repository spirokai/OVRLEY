import { memo, useMemo } from 'react'
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
  getDistanceProgress,
  getPointAtProgress,
  getPreviewFontFamily,
  getSampleValue,
  getWidgetOpacity,
  normalizeElevationPoints,
  normalizeRoutePoints,
  pointsToSvg,
} from './utils'

function pointsEqual(left, right) {
  return left?.[0] === right?.[0] && left?.[1] === right?.[1]
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
  const points = useMemo(
    () =>
      normalizeRoutePoints(activity?.sample_course_points || [], width, height),
    [activity?.sample_course_points, width, height],
  )
  const progress01 = getDistanceProgress(activity, sampleIndex)
  const completedIndex = getCompletedIndex(
    points.length,
    sampleIndex,
    progress01,
  )
  const markerPoint =
    getPointAtProgress(points, progress01) ||
    points[completedIndex] ||
    points[points.length - 1]
  const completedPoints = useMemo(() => {
    const nextPoints = points.slice(0, completedIndex + 1)
    if (
      markerPoint &&
      !pointsEqual(nextPoints[nextPoints.length - 1], markerPoint)
    ) {
      nextPoints.push(markerPoint)
    }
    return nextPoints
  }, [completedIndex, markerPoint, points])
  const remainingPoints = useMemo(() => {
    const tailStart = Math.min(completedIndex + 1, points.length - 1)
    const tail = points.slice(tailStart)

    if (!markerPoint) {
      return tail.length ? tail : points.slice(completedIndex)
    }

    return pointsEqual(markerPoint, tail[0]) ? tail : [markerPoint, ...tail]
  }, [completedIndex, markerPoint, points])
  const remainingSvgPoints = useMemo(
    () => pointsToSvg(remainingPoints.length > 1 ? remainingPoints : points),
    [points, remainingPoints],
  )
  const completedSvgPoints = useMemo(
    () =>
      pointsToSvg(
        completedPoints.length > 1 ? completedPoints : points.slice(0, 2),
      ),
    [completedPoints, points],
  )

  return (
    <svg
      width={width}
      height={height}
      viewBox={`0 0 ${width} ${height}`}
      className="block h-full w-full"
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
          points={remainingSvgPoints}
        />
        <polyline
          fill="none"
          stroke={widget.data.completed_line_color || '#afeeee'}
          strokeOpacity={(widget.data.completed_line_opacity ?? 100) / 100}
          strokeWidth={widget.data.completed_line_width ?? 6}
          strokeLinejoin="round"
          strokeLinecap="round"
          points={completedSvgPoints}
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
  const clipId = `${widget.id}-completed-area`
  const remainingAreaColor =
    widget.data.area_remaining_color ||
    widget.data.remaining_line_color ||
    '#005b5b'
  const completedAreaColor =
    widget.data.area_completed_color ||
    widget.data.completed_line_color ||
    '#afeeee'
  const remainingAreaOpacity = (widget.data.area_remaining_opacity ?? 12) / 100
  const completedAreaOpacity = (widget.data.area_completed_opacity ?? 24) / 100
  const points = useMemo(
    () =>
      normalizeElevationPoints(
        activity?.sample_elevations || [],
        width,
        height,
        18,
        widget.data.y_scale ?? 1,
      ),
    [activity?.sample_elevations, height, widget.data.y_scale, width],
  )
  const progress01 = getDistanceProgress(activity, sampleIndex)
  const completedIndex = getCompletedIndex(
    points.length,
    sampleIndex,
    progress01,
  )
  const markerPoint =
    getPointAtProgress(points, progress01) ||
    points[completedIndex] ||
    points[points.length - 1]
  const completedPoints = useMemo(() => {
    const nextPoints = points.slice(0, completedIndex + 1)
    if (
      markerPoint &&
      !pointsEqual(nextPoints[nextPoints.length - 1], markerPoint)
    ) {
      nextPoints.push(markerPoint)
    }
    return nextPoints
  }, [completedIndex, markerPoint, points])
  const remainingPoints = useMemo(() => {
    const tailStart = Math.min(completedIndex + 1, points.length - 1)
    const tail = points.slice(tailStart)

    if (!markerPoint) {
      return tail.length ? tail : points.slice(completedIndex)
    }

    return pointsEqual(markerPoint, tail[0]) ? tail : [markerPoint, ...tail]
  }, [completedIndex, markerPoint, points])
  const elevationValue = getSampleValue(activity, 'elevation', sampleIndex)
  const areaSvgPoints = useMemo(
    () => areaToSvg(points, width, height),
    [height, points, width],
  )
  const completedAreaWidth = markerPoint ? markerPoint[0] : 0
  const remainingSvgPoints = useMemo(
    () => pointsToSvg(remainingPoints.length > 1 ? remainingPoints : points),
    [points, remainingPoints],
  )
  const completedSvgPoints = useMemo(
    () =>
      pointsToSvg(
        completedPoints.length > 1 ? completedPoints : points.slice(0, 2),
      ),
    [completedPoints, points],
  )
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
      <svg
        width={width}
        height={height}
        viewBox={`0 0 ${width} ${height}`}
        className="block h-full w-full"
      >
        <defs>
          <clipPath id={clipId}>
            <rect x="0" y="0" width={completedAreaWidth} height={height} />
          </clipPath>
        </defs>
        <polygon
          points={areaSvgPoints}
          fill={remainingAreaColor}
          fillOpacity={remainingAreaOpacity}
        />
        <polygon
          points={areaSvgPoints}
          fill={completedAreaColor}
          fillOpacity={completedAreaOpacity}
          clipPath={`url(#${clipId})`}
        />
        <polyline
          fill="none"
          stroke={widget.data.remaining_line_color || '#005b5b'}
          strokeOpacity={(widget.data.remaining_line_opacity ?? 35) / 100}
          strokeWidth={widget.data.remaining_line_width ?? 6}
          strokeLinejoin="round"
          strokeLinecap="round"
          points={remainingSvgPoints}
        />
        <polyline
          fill="none"
          stroke={widget.data.completed_line_color || '#afeeee'}
          strokeOpacity={(widget.data.completed_line_opacity ?? 100) / 100}
          strokeWidth={widget.data.completed_line_width ?? 6}
          strokeLinejoin="round"
          strokeLinecap="round"
          points={completedSvgPoints}
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

export default memo(
  WidgetPreview,
  (previousProps, nextProps) =>
    previousProps.widget === nextProps.widget &&
    previousProps.activity === nextProps.activity &&
    previousProps.sampleIndex === nextProps.sampleIndex &&
    previousProps.globalOpacity === nextProps.globalOpacity,
)
