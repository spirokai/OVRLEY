import { memo, useMemo } from 'react'
import { DEFAULT_GRADIENT_TRIANGLE_WIDTH, WIDGET_ICONS } from './constants'
import {
  areaToSvg,
  buildGradientTrianglePath,
  clamp,
  formatGradientValue,
  formatSpeed,
  formatTemperature,
  formatTimeValue,
  getCombinedTextShadow,
  getDistanceProgressAtElapsed,
  getInterpolatedActivityValue,
  getInterpolatedSeriesValue,
  getInterpolatedTimeValue,
  getPointAtMetricProgress,
  getPointAtX,
  getPointAtProgress,
  getPreviewFontFamily,
  getSeriesValueAtProgress,
  getWidgetOpacity,
  normalizeElevationPoints,
  normalizeRoutePoints,
  pointsToSvg,
} from './utils'

function pointsEqual(left, right) {
  return left?.[0] === right?.[0] && left?.[1] === right?.[1]
}

function OverlayMetricWidget({
  widget,
  activity,
  previewSecond,
  globalOpacity,
}) {
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
      getInterpolatedActivityValue(activity, 'speed', previewSecond),
      speedUnit,
    )
    valueText = formatted.value
    unitText = formatted.units
  } else if (widget.type === 'heartrate') {
    const value = getInterpolatedActivityValue(
      activity,
      'heartrate',
      previewSecond,
    )
    valueText =
      value === null || value === undefined
        ? '--'
        : Math.round(value).toString()
    unitText = 'BPM'
  } else if (widget.type === 'cadence') {
    const value = getInterpolatedActivityValue(
      activity,
      'cadence',
      previewSecond,
    )
    valueText =
      value === null || value === undefined
        ? '--'
        : Math.round(value).toString()
    unitText = 'RPM'
  } else if (widget.type === 'power') {
    const value = getInterpolatedActivityValue(activity, 'power', previewSecond)
    valueText =
      value === null || value === undefined
        ? '--'
        : Math.round(value).toString()
    unitText = 'W'
  } else if (widget.type === 'time') {
    valueText = formatTimeValue(
      widget.data.format || 'time-24',
      getInterpolatedTimeValue(activity, previewSecond),
    )
  } else if (widget.type === 'temperature') {
    const formatted = formatTemperature(
      getInterpolatedActivityValue(activity, 'temperature', previewSecond),
      widget.data.temperature_unit || 'celsius',
    )
    valueText = formatted.value
    unitText = formatted.units
  } else if (widget.type === 'gradient') {
    valueText = `${formatGradientValue(
      widget,
      getInterpolatedActivityValue(activity, 'gradient', previewSecond),
    )}%`
  }

  const currentGradientValue = Number(
    getInterpolatedActivityValue(activity, 'gradient', previewSecond) ?? 0,
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

function OverlayRouteWidget({
  widget,
  activity,
  previewSecond,
  globalOpacity,
}) {
  const width = Math.max(widget.data.width ?? 320, 80)
  const height = Math.max(widget.data.height ?? 180, 80)
  const routeSamples = useMemo(() => {
    const coursePoints = Array.isArray(activity?.sample_course_points)
      ? activity.sample_course_points
      : []
    const distanceProgress = Array.isArray(activity?.sample_distance_progress)
      ? activity.sample_distance_progress
      : []

    return coursePoints.reduce((result, point, index) => {
      if (
        !Array.isArray(point) ||
        !Number.isFinite(point[0]) ||
        !Number.isFinite(point[1])
      ) {
        return result
      }

      result.push({
        point,
        progress: clamp(Number(distanceProgress[index]) || 0, 0, 1),
      })
      return result
    }, [])
  }, [activity])
  const points = useMemo(
    () =>
      normalizeRoutePoints(
        routeSamples.map((sample) => sample.point),
        width,
        height,
      ),
    [routeSamples, width, height],
  )
  const pointProgress = useMemo(
    () => routeSamples.map((sample) => sample.progress),
    [routeSamples],
  )
  const progress01 = getDistanceProgressAtElapsed(activity, previewSecond)
  const markerPoint =
    getPointAtMetricProgress(points, pointProgress, progress01) ||
    getPointAtProgress(points, progress01) ||
    points[points.length - 1]
  const completedPoints = useMemo(() => {
    const nextPoints = points.filter(
      (_, index) => (pointProgress[index] ?? 0) <= progress01,
    )
    if (
      markerPoint &&
      !pointsEqual(nextPoints[nextPoints.length - 1], markerPoint)
    ) {
      nextPoints.push(markerPoint)
    }
    return nextPoints
  }, [markerPoint, pointProgress, points, progress01])
  const remainingPoints = useMemo(() => {
    const tail = points.filter(
      (_, index) => (pointProgress[index] ?? 0) >= progress01,
    )

    if (!markerPoint) {
      return tail.length ? tail : points
    }

    return pointsEqual(markerPoint, tail[0]) ? tail : [markerPoint, ...tail]
  }, [markerPoint, pointProgress, points, progress01])
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
  previewSecond,
  globalOpacity,
}) {
  const PROFILE_PADDING = 18
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
  const profileElevations = useMemo(() => {
    if (
      Array.isArray(activity?.sample_elevations) &&
      activity.sample_elevations.length
    ) {
      return activity.sample_elevations
    }

    return Array.isArray(activity?.elevation) ? activity.elevation : []
  }, [activity])
  const profileDistanceProgress = useMemo(
    () =>
      Array.isArray(activity?.sample_distance_progress)
        ? activity.sample_distance_progress
        : [],
    [activity],
  )
  const points = useMemo(
    () =>
      normalizeElevationPoints(
        profileElevations,
        width,
        height,
        PROFILE_PADDING,
        widget.data.y_scale ?? 1,
        profileDistanceProgress,
        widget.data.target_density ?? 0.75,
        widget.data.simplify_tolerance_px ?? 1,
      ),
    [
      PROFILE_PADDING,
      height,
      profileDistanceProgress,
      profileElevations,
      widget.data.simplify_tolerance_px,
      widget.data.target_density,
      widget.data.y_scale,
      width,
    ],
  )
  const progress01 = getDistanceProgressAtElapsed(activity, previewSecond)
  const markerX =
    PROFILE_PADDING + progress01 * Math.max(width - PROFILE_PADDING * 2, 0)
  const markerPoint = getPointAtX(points, markerX) || points[points.length - 1]
  const completedPoints = points.filter((point) => point[0] <= markerX)
  if (
    markerPoint &&
    !pointsEqual(completedPoints[completedPoints.length - 1], markerPoint)
  ) {
    completedPoints.push(markerPoint)
  }

  const remainingTail = points.filter((point) => point[0] >= markerX)
  const remainingPoints =
    !markerPoint || pointsEqual(markerPoint, remainingTail[0])
      ? remainingTail
      : [markerPoint, ...remainingTail]
  const elevationValue =
    getInterpolatedSeriesValue(
      profileDistanceProgress,
      profileElevations,
      progress01,
    ) ?? getSeriesValueAtProgress(profileElevations, progress01)
  const areaSvgPoints = useMemo(
    () => areaToSvg(points, width, height, PROFILE_PADDING),
    [PROFILE_PADDING, height, points, width],
  )
  const completedAreaWidth = markerPoint ? markerPoint[0] : 0
  const remainingSvgPoints = pointsToSvg(
    remainingPoints.length > 1 ? remainingPoints : points,
  )
  const completedSvgPoints = pointsToSvg(
    completedPoints.length > 1 ? completedPoints : points.slice(0, 2),
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

function WidgetPreview({ widget, activity, previewSecond, globalOpacity }) {
  if (widget.type === 'label') {
    return <OverlayTextWidget widget={widget} globalOpacity={globalOpacity} />
  }

  if (widget.type === 'course') {
    return (
      <OverlayRouteWidget
        widget={widget}
        activity={activity}
        previewSecond={previewSecond}
        globalOpacity={globalOpacity}
      />
    )
  }

  if (widget.type === 'elevation') {
    return (
      <OverlayElevationWidget
        widget={widget}
        activity={activity}
        previewSecond={previewSecond}
        globalOpacity={globalOpacity}
      />
    )
  }

  return (
    <OverlayMetricWidget
      widget={widget}
      activity={activity}
      previewSecond={previewSecond}
      globalOpacity={globalOpacity}
    />
  )
}

export default memo(
  WidgetPreview,
  (previousProps, nextProps) =>
    previousProps.widget === nextProps.widget &&
    previousProps.activity === nextProps.activity &&
    previousProps.previewSecond === nextProps.previewSecond &&
    previousProps.globalOpacity === nextProps.globalOpacity,
)
