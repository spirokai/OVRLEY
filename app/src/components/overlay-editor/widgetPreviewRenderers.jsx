/**
 * Provides overlay editor helpers for widget preview renderers.
 */

import { useEffect, useMemo, useState } from 'react'
import useStore from '@/store/useStore'
import {
  buildScopedElevationSeries,
  buildScopedRouteSamples,
  getExportWindowDistanceProgressAtElapsed,
  resolveExportRangeWindow,
} from '@/lib/export-range'
import { DEFAULT_GRADIENT_TRIANGLE_WIDTH } from './constants'
import { METRIC_ICON_SVGS } from './metricWidgetAssets'
import {
  areaToSvg,
  getPointAtMetricProgress,
  getPointAtProgress,
  normalizeElevationGeometry,
  normalizeRouteGeometry,
  pointsToSvg,
} from './geometryUtils'
import {
  buildGradientTrianglePath,
  formatGradientValue,
  getGradientWidgetLayout,
  GRADIENT_ZERO_LINE_WIDTH_PX,
  formatSpeed,
  formatTemperature,
  formatTimeValue,
  getMetricWidgetLayout,
  getPreviewFontFamily,
  getPreviewTextBaseline,
  getTextShadow,
  getTextShadowParts,
  getWidgetOpacity,
  METRIC_WIDGET_LINE_HEIGHT,
  measurePreviewText,
} from './metricTextUtils'
import {
  getDistanceProgressAtElapsed,
  getInterpolatedActivityValue,
  getInterpolatedSeriesValue,
  getInterpolatedTimeValue,
  getSeriesValueAtProgress,
} from './utils'

/**
 * Handles points equal.
 *
 * @param {*} left - Left-hand comparison value.
 * @param {*} right - Right-hand comparison value.
 * @returns {*} Result produced by the helper.
 */
function pointsEqual(left, right) {
  return left?.[0] === right?.[0] && left?.[1] === right?.[1]
}

/**
 * Returns a sanitized svg id fragment.
 *
 * @param {*} value - Input value processed by the helper.
 * @returns {string} Sanitized svg id fragment.
 */
function sanitizeSvgId(value) {
  return String(value || 'preview-shadow').replace(/[^a-zA-Z0-9_-]/g, '_')
}

/**
 * Returns a version token that changes after the requested font becomes ready.
 *
 * @param {*} fontFamily - Font family used by the preview widget.
 * @param {*} fontSize - Numeric font size value.
 * @returns {number} Version token for preview re-measurement.
 */
function useFontMetricsVersion(fontFamily, fontSize) {
  const [version, setVersion] = useState(0)

  useEffect(() => {
    if (
      typeof document === 'undefined' ||
      !document.fonts ||
      typeof document.fonts.load !== 'function'
    ) {
      return undefined
    }

    let cancelled = false

    const refreshMetrics = async () => {
      try {
        await Promise.allSettled([
          document.fonts.load(
            `${fontSize}px ${fontFamily}`,
            '0123456789WBMPRK/H',
          ),
          document.fonts.ready,
        ])
      } finally {
        if (!cancelled) {
          setVersion((current) => current + 1)
        }
      }
    }

    refreshMetrics()

    return () => {
      cancelled = true
    }
  }, [fontFamily, fontSize])

  return version
}

/**
 * Renders the preview svg text component.
 *
 * @param {object} props - Component props.
 * @param {*} props.text - Text content to measure or render.
 * @param {*} props.x - Horizontal coordinate.
 * @param {*} props.baseline - Value for baseline.
 * @param {*} props.color - Value for color.
 * @param {*} props.fontFamily - Font family used for measurement or rendering.
 * @param {*} props.fontSize - Numeric font size value.
 * @param {*} props.opacity - Value for opacity.
 * @param {*} props.shadow - Structured shadow data for svg filter rendering.
 * @param {*} props.shadowFilterId - Stable filter id for the text shadow.
 * @param {*} props.borderColor - Value for border color.
 * @param {*} props.borderThickness - Value for border thickness.
 * @returns {JSX.Element} Rendered component output.
 */
function PreviewSvgText({
  text,
  x = 0,
  baseline,
  color,
  fontFamily,
  fontSize,
  opacity,
  shadow,
  shadowFilterId,
  borderColor,
  borderThickness,
}) {
  const hasShadow = Boolean(shadow && shadowFilterId)

  return (
    <>
      {hasShadow ? (
        <defs>
          <filter
            id={shadowFilterId}
            x="-50%"
            y="-50%"
            width="200%"
            height="200%"
            colorInterpolationFilters="sRGB"
          >
            <feDropShadow
              dx={shadow.distance}
              dy={shadow.distance}
              stdDeviation={shadow.strength}
              floodColor={shadow.color}
            />
          </filter>
        </defs>
      ) : null}
      <text
        x={x}
        y={baseline}
        fill={color}
        fontFamily={fontFamily}
        fontSize={fontSize}
        opacity={opacity}
        paintOrder="stroke fill"
        stroke={borderColor || 'none'}
        strokeWidth={borderThickness || 0}
        filter={hasShadow ? `url(#${shadowFilterId})` : undefined}
      >
        {text}
      </text>
    </>
  )
}

/**
 * Renders the overlay metric widget component.
 *
 * @param {object} props - Component props.
 * @param {*} props.widget - Widget definition being rendered or edited.
 * @param {*} props.activity - Parsed activity data for previews or rendering.
 * @param {*} props.previewSecond - Preview time in seconds.
 * @param {*} props.globalOpacity - Global opacity multiplier applied to the widget.
 * @returns {JSX.Element} Rendered component output.
 */
export function OverlayMetricWidget({
  widget,
  activity,
  previewSecond,
  globalOpacity,
}) {
  const fontSize = widget.data.font_size ?? 60
  const fontFamily = getPreviewFontFamily(
    widget.data.font || widget.data.font_family,
  )
  const fontMetricsVersion = useFontMetricsVersion(fontFamily, fontSize)
  const color = widget.data.color || '#ffffff'
  const widgetOpacity = getWidgetOpacity(widget.data, globalOpacity)
  const textShadow = getTextShadow(widget.data)
  const shadow = getTextShadowParts(widget.data)

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
  const showUnits =
    widget.data.show_units ?? ['speed', 'temperature'].includes(widget.type)
  const showIcon = widget.data.show_icon ?? widget.type !== 'gradient'
  const metricLayout = useMemo(
    () =>
      widget.type === 'gradient'
        ? null
        : getMetricWidgetLayout({
            fontSize,
            fontFamily,
            valueText,
            unitText,
            showIcon: Boolean(showIcon && METRIC_ICON_SVGS[widget.type]),
            showUnits,
            iconSize,
          }),
    [
      fontFamily,
      fontMetricsVersion,
      fontSize,
      iconSize,
      showIcon,
      showUnits,
      unitText,
      valueText,
      widget.type,
    ],
  )
  const gradientLayout = useMemo(
    () =>
      widget.type === 'gradient'
        ? getGradientWidgetLayout({
            fontSize,
            fontFamily,
            valueText,
            valueOffset: widget.data.value_offset ?? 0,
            gradientValue: currentGradientValue,
            triangleWidth:
              widget.data.triangle_width ?? DEFAULT_GRADIENT_TRIANGLE_WIDTH,
            showTriangle: widget.data.show_triangle !== false,
          })
        : null,
    [
      currentGradientValue,
      fontFamily,
      fontSize,
      valueText,
      widget.data.show_triangle,
      widget.data.triangle_width,
      widget.data.value_offset,
      widget.type,
    ],
  )

  if (widget.type !== 'gradient' && metricLayout) {
    const iconSvg = METRIC_ICON_SVGS[widget.type]
    const valueShadowFilterId = sanitizeSvgId(`${widget.id}-value-shadow`)
    const unitsShadowFilterId = sanitizeSvgId(`${widget.id}-units-shadow`)
    const iconLeft = metricLayout.icon
      ? metricLayout.icon.left + (widget.data.icon_offset_x ?? 0)
      : 0
    const iconTop = metricLayout.icon
      ? metricLayout.icon.top + (widget.data.icon_offset_y ?? 0)
      : 0
    const minX = metricLayout.icon ? Math.min(0, iconLeft) : 0
    const minY = metricLayout.icon ? Math.min(0, iconTop) : 0
    const maxX = metricLayout.icon
      ? Math.max(metricLayout.width, iconLeft + metricLayout.icon.size)
      : metricLayout.width
    const maxY = metricLayout.icon
      ? Math.max(metricLayout.height, iconTop + metricLayout.icon.size)
      : metricLayout.height

    return (
      <div
        className="relative"
        style={{
          width: maxX - minX,
          height: maxY - minY,
        }}
      >
        <div
          className="absolute"
          style={{
            left: -minX,
            top: -minY,
            width: metricLayout.width,
            height: metricLayout.height,
          }}
        >
          {metricLayout.icon && iconSvg ? (
            <div
              className="metric-icon absolute"
              style={{
                left: iconLeft,
                top: iconTop,
                width: metricLayout.icon.size,
                height: metricLayout.icon.size,
                color: widget.data.icon_color || '#40e0d0',
                opacity: widgetOpacity,
              }}
              dangerouslySetInnerHTML={{ __html: iconSvg }}
            />
          ) : null}
          <svg
            width={metricLayout.width}
            height={metricLayout.height}
            viewBox={`0 0 ${metricLayout.width} ${metricLayout.height}`}
            className="absolute left-0 top-0 block overflow-visible"
          >
            <PreviewSvgText
              text={valueText}
              x={metricLayout.value.left}
              baseline={metricLayout.value.baseline}
              color={color}
              fontFamily={fontFamily}
              fontSize={fontSize}
              opacity={widgetOpacity}
              shadow={shadow}
              shadowFilterId={valueShadowFilterId}
              borderColor={widget.data.border_color}
              borderThickness={widget.data.border_thickness}
            />
            {metricLayout.units ? (
              <PreviewSvgText
                text={unitText}
                x={metricLayout.units.left}
                baseline={metricLayout.units.baseline}
                color={color}
                fontFamily={fontFamily}
                fontSize={metricLayout.units.fontSize}
                opacity={widgetOpacity}
                shadow={shadow}
                shadowFilterId={unitsShadowFilterId}
                borderColor={widget.data.border_color}
                borderThickness={widget.data.border_thickness}
              />
            ) : null}
          </svg>
        </div>
      </div>
    )
  }

  if (widget.type === 'gradient' && gradientLayout) {
    const valueShadowFilterId = sanitizeSvgId(`${widget.id}-value-shadow`)
    const trianglePath = gradientLayout.triangle
      ? buildGradientTrianglePath(
          currentGradientValue,
          gradientLayout.triangle.width,
          gradientLayout.triangle.height,
        )
      : ''

    return (
      <svg
        width={gradientLayout.width}
        height={gradientLayout.height}
        viewBox={`0 0 ${gradientLayout.width} ${gradientLayout.height}`}
        className="block overflow-visible"
      >
        <PreviewSvgText
          text={valueText}
          x={gradientLayout.value.left}
          baseline={gradientLayout.value.baseline}
          color={color}
          fontFamily={fontFamily}
          fontSize={fontSize}
          opacity={widgetOpacity}
          shadow={shadow}
          shadowFilterId={valueShadowFilterId}
          borderColor={widget.data.border_color}
          borderThickness={widget.data.border_thickness}
        />
        {gradientLayout.triangle ? (
          gradientLayout.triangle.isZero ? (
            <line
              x1={gradientLayout.triangle.left}
              y1={gradientLayout.triangle.baseline}
              x2={gradientLayout.triangle.left + gradientLayout.triangle.width}
              y2={gradientLayout.triangle.baseline}
              stroke={widget.data.triangle_positive_color || '#40e0d0'}
              strokeWidth={GRADIENT_ZERO_LINE_WIDTH_PX}
              opacity={widgetOpacity}
              strokeLinecap="round"
            />
          ) : trianglePath ? (
            <path
              d={trianglePath}
              transform={`translate(${gradientLayout.triangle.left} ${gradientLayout.triangle.baseline})`}
              fill={
                currentGradientValue < 0
                  ? widget.data.triangle_negative_color || '#c65102'
                  : widget.data.triangle_positive_color || '#40e0d0'
              }
              opacity={widgetOpacity}
            />
          ) : null
        ) : null}
      </svg>
    )
  }

  return (
    <div
      className="inline-flex items-center gap-2 whitespace-nowrap"
      style={{
        color,
        fontFamily,
        fontSize,
        lineHeight: METRIC_WIDGET_LINE_HEIGHT,
        opacity: widgetOpacity,
        textShadow,
        transform: `translateY(${widget.data.value_offset ?? 0}px)`,
      }}
    >
      <div className="inline-flex items-end gap-2">
        <span>{valueText}</span>
        {showUnits && unitText ? (
          <span
            style={{
              fontSize: Math.max(fontSize * 0.28, 12),
              lineHeight: METRIC_WIDGET_LINE_HEIGHT,
              opacity: 'inherit',
            }}
          >
            {unitText}
          </span>
        ) : null}
      </div>
    </div>
  )
}

/**
 * Renders the overlay text widget component.
 *
 * @param {object} props - Component props.
 * @param {*} props.widget - Widget definition being rendered or edited.
 * @param {*} props.globalOpacity - Global opacity multiplier applied to the widget.
 * @returns {JSX.Element} Rendered component output.
 */
export function OverlayTextWidget({ widget, globalOpacity }) {
  const fontSize = widget.data.font_size ?? 60
  const fontFamily = getPreviewFontFamily(
    widget.data.font || widget.data.font_family,
  )
  const fontMetricsVersion = useFontMetricsVersion(fontFamily, fontSize)
  const color = widget.data.color || '#ffffff'
  const opacity = getWidgetOpacity(widget.data, globalOpacity)
  const shadow = getTextShadowParts(widget.data)
  const text = widget.data.text || 'TEXT'
  const lineHeight = fontSize * METRIC_WIDGET_LINE_HEIGHT
  const measurement = useMemo(
    () => measurePreviewText(text, fontSize, fontFamily),
    [fontFamily, fontMetricsVersion, fontSize, text],
  )
  const baseline = getPreviewTextBaseline({
    top: 0,
    lineHeight,
    ascent: measurement.ascent,
    descent: measurement.descent,
    glyphHeight: measurement.glyphHeight,
  })

  return (
    <svg
      width={measurement.width}
      height={lineHeight}
      viewBox={`0 0 ${measurement.width} ${lineHeight}`}
      className="block overflow-visible"
    >
      <PreviewSvgText
        text={text}
        baseline={baseline}
        color={color}
        fontFamily={fontFamily}
        fontSize={fontSize}
        opacity={opacity}
        shadow={shadow}
        shadowFilterId={sanitizeSvgId(`${widget.id}-label-shadow`)}
        borderColor={widget.data.border_color}
        borderThickness={widget.data.border_thickness}
      />
    </svg>
  )
}

/**
 * Renders the overlay route widget component.
 *
 * @param {object} props - Component props.
 * @param {*} props.widget - Widget definition being rendered or edited.
 * @param {*} props.activity - Parsed activity data for previews or rendering.
 * @param {*} props.previewSecond - Preview time in seconds.
 * @param {*} props.globalOpacity - Global opacity multiplier applied to the widget.
 * @returns {JSX.Element} Rendered component output.
 */
export function OverlayRouteWidget({
  widget,
  activity,
  previewSecond,
  globalOpacity,
}) {
  const width = Math.max(widget.data.width ?? 320, 80)
  const height = Math.max(widget.data.height ?? 180, 80)
  const exportRange = useStore((state) => state.exportRange)
  const exportWindow = useMemo(
    () =>
      resolveExportRangeWindow(
        activity,
        exportRange,
        widget.data.show_full_activity ?? false,
      ),
    [activity, exportRange, widget.data.show_full_activity],
  )
  const routeSamples = useMemo(() => {
    return buildScopedRouteSamples(activity, exportWindow)
  }, [activity, exportWindow])
  const routeGeometry = useMemo(
    () =>
      normalizeRouteGeometry(
        routeSamples,
        width,
        height,
        widget.data.target_density ?? 1,
        widget.data.simplify_tolerance_px ?? 1,
        widget.data.remaining_line_width ?? 6,
        widget.data.completed_line_width ?? 6,
        widget.data.marker_size ?? 18,
      ),
    [
      height,
      routeSamples,
      width,
      widget.data.completed_line_width,
      widget.data.marker_size,
      widget.data.remaining_line_width,
      widget.data.simplify_tolerance_px,
      widget.data.target_density,
    ],
  )
  const pointProgress = routeGeometry.progressValues
  const progress01 = exportWindow.active
    ? (getExportWindowDistanceProgressAtElapsed(
        activity,
        exportWindow,
        previewSecond,
      ) ?? 0)
    : getDistanceProgressAtElapsed(activity, previewSecond)
  const markerPoint =
    getPointAtMetricProgress(routeGeometry.points, pointProgress, progress01) ||
    getPointAtProgress(routeGeometry.points, progress01) ||
    routeGeometry.points[routeGeometry.points.length - 1]
  const completedPoints = useMemo(() => {
    const nextPoints = routeGeometry.points.filter(
      (_, index) => (pointProgress[index] ?? 0) <= progress01,
    )
    if (
      markerPoint &&
      !pointsEqual(nextPoints[nextPoints.length - 1], markerPoint)
    ) {
      nextPoints.push(markerPoint)
    }
    return nextPoints
  }, [markerPoint, pointProgress, progress01, routeGeometry.points])
  const remainingPoints = useMemo(() => {
    const tail = routeGeometry.points.filter(
      (_, index) => (pointProgress[index] ?? 0) >= progress01,
    )

    if (!markerPoint) {
      return tail.length ? tail : routeGeometry.points
    }

    return pointsEqual(markerPoint, tail[0]) ? tail : [markerPoint, ...tail]
  }, [markerPoint, pointProgress, progress01, routeGeometry.points])
  const remainingSvgPoints = useMemo(
    () =>
      pointsToSvg(
        remainingPoints.length > 1 ? remainingPoints : routeGeometry.points,
      ),
    [remainingPoints, routeGeometry.points],
  )
  const completedSvgPoints = useMemo(
    () => pointsToSvg(completedPoints),
    [completedPoints],
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

/**
 * Renders the overlay elevation widget component.
 *
 * @param {object} props - Component props.
 * @param {*} props.widget - Widget definition being rendered or edited.
 * @param {*} props.activity - Parsed activity data for previews or rendering.
 * @param {*} props.previewSecond - Preview time in seconds.
 * @param {*} props.globalOpacity - Global opacity multiplier applied to the widget.
 * @returns {JSX.Element} Rendered component output.
 */
export function OverlayElevationWidget({
  widget,
  activity,
  previewSecond,
  globalOpacity,
}) {
  const profilePadding = 18
  const width = Math.max(widget.data.width ?? 320, 80)
  const height = Math.max(widget.data.height ?? 180, 80)
  const clipId = `${widget.id}-completed-area`
  const exportRange = useStore((state) => state.exportRange)
  const exportWindow = useMemo(
    () =>
      resolveExportRangeWindow(
        activity,
        exportRange,
        widget.data.show_full_activity ?? false,
      ),
    [activity, exportRange, widget.data.show_full_activity],
  )
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
  const scopedElevationSeries = useMemo(
    () => buildScopedElevationSeries(activity, exportWindow),
    [activity, exportWindow],
  )
  const profileElevations = scopedElevationSeries.values
  const profileDistanceProgress = scopedElevationSeries.progressValues
  const elevationGeometry = useMemo(
    () =>
      normalizeElevationGeometry(
        profileElevations,
        width,
        height,
        profilePadding,
        widget.data.y_scale ?? 1,
        profileDistanceProgress,
        widget.data.target_density ?? 0.75,
        widget.data.simplify_tolerance_px ?? 1,
      ),
    [
      height,
      profileDistanceProgress,
      profileElevations,
      profilePadding,
      width,
      widget.data.simplify_tolerance_px,
      widget.data.target_density,
      widget.data.y_scale,
    ],
  )
  const points = elevationGeometry.points
  const pointProgress = elevationGeometry.progressValues
  const progress01 = exportWindow.active
    ? (getExportWindowDistanceProgressAtElapsed(
        activity,
        exportWindow,
        previewSecond,
      ) ?? 0)
    : getDistanceProgressAtElapsed(activity, previewSecond)
  const markerPoint =
    getPointAtMetricProgress(points, pointProgress, progress01) ||
    getPointAtProgress(points, progress01) ||
    points[points.length - 1]
  const completedPoints = points.filter(
    (_, index) => (pointProgress[index] ?? 0) <= progress01,
  )
  if (
    markerPoint &&
    !pointsEqual(completedPoints[completedPoints.length - 1], markerPoint)
  ) {
    completedPoints.push(markerPoint)
  }

  const remainingTail = points.filter(
    (_, index) => (pointProgress[index] ?? 0) >= progress01,
  )
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
    () => areaToSvg(points, width, height, profilePadding),
    [height, points, profilePadding, width],
  )
  const completedAreaWidth = markerPoint
    ? markerPoint[0]
    : (points[0]?.[0] ?? 0)
  const remainingSvgPoints = pointsToSvg(
    remainingPoints.length > 1 ? remainingPoints : points,
  )
  const completedSvgPoints = pointsToSvg(completedPoints)
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
