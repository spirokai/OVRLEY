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
  getPointAtMetricProgressWithIndex,
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
  if (!left || !right) {
    return false
  }

  return Math.hypot(right[0] - left[0], right[1] - left[1]) <= 1e-3
}

/**
 * Normalizes opacity values the same way as the Skia renderer.
 *
 * @param {*} value - Raw opacity value.
 * @param {*} fallback - Fallback opacity.
 * @returns {number} Normalized opacity.
 */
function normalizePreviewOpacity(value, fallback) {
  if (value === null || value === undefined) {
    return fallback
  }

  const numericValue = Number(value)
  if (!Number.isFinite(numericValue)) {
    return fallback
  }

  return numericValue > 1
    ? Math.min(Math.max(numericValue / 100, 0), 1)
    : Math.min(Math.max(numericValue, 0), 1)
}

/**
 * Resolves plot style color following backend inheritance order.
 *
 * @param {*} explicitColor - Explicit plot color.
 * @param {*} inheritedColor - Nested legacy style color.
 * @param {*} baseColor - Base plot color.
 * @returns {string} Resolved color.
 */
function resolvePreviewStyleColor(explicitColor, inheritedColor, baseColor) {
  return explicitColor || inheritedColor || baseColor || '#ffffff'
}

/**
 * Resolves legacy line width for preview plot widgets.
 *
 * @param {*} explicitWidth - Explicit width.
 * @param {*} legacyWidth - Legacy line style width.
 * @returns {number} Resolved line width.
 */
function resolvePreviewLineWidth(explicitWidth, legacyWidth) {
  const numericExplicit = Number(explicitWidth)
  if (Number.isFinite(numericExplicit)) {
    return numericExplicit
  }

  const numericLegacy = Number(legacyWidth)
  return (Number.isFinite(numericLegacy) ? numericLegacy : 1.75) * 2.5
}

/**
 * Resolves SVG stroke width under the editor's outer CSS scale.
 *
 * @param {*} explicitWidth - Explicit plot stroke width.
 * @param {*} legacyWidth - Legacy line style width.
 * @param {*} globalScale - Scene scale applied by the overlay wrapper.
 * @returns {number} SVG-local stroke width.
 */
function resolveScaledPreviewLineWidth(
  explicitWidth,
  legacyWidth,
  globalScale,
) {
  const safeScale = Math.max(Number(globalScale) || 1, 0.1)
  const numericExplicit = Number(explicitWidth)

  if (Number.isFinite(numericExplicit)) {
    return numericExplicit / safeScale
  }

  return resolvePreviewLineWidth(undefined, legacyWidth)
}

/**
 * Builds marker layers like the Skia renderer.
 *
 * @param {*} widgetData - Plot widget data.
 * @param {*} fallbackRadius - Fallback radius.
 * @param {*} fallbackColor - Fallback color.
 * @param {*} fallbackOpacity - Fallback opacity.
 * @returns {Array} Marker layers.
 */
function getPreviewMarkerLayers(
  widgetData,
  fallbackRadius,
  fallbackColor,
  fallbackOpacity,
) {
  const sourcePoints = Array.isArray(widgetData.points) ? widgetData.points : []
  const markerPoints = sourcePoints.length
    ? sourcePoints
    : [
        {
          weight: fallbackRadius ** 2,
          color: fallbackColor,
          opacity: fallbackOpacity,
        },
      ]

  return markerPoints
    .map((point) => ({
      radius: Math.max(Math.sqrt(Math.max(Number(point.weight) || 80, 1)), 2),
      color: point.color || '#ffffff',
      opacity: normalizePreviewOpacity(point.opacity, 1),
    }))
    .sort((left, right) => right.radius - left.radius)
    .map((layer, index, layers) => ({
      ...layer,
      solidFill: index === layers.length - 1,
    }))
}

/**
 * Renders Skia-compatible marker layers.
 *
 * @param {object} props - Component props.
 * @param {*} props.layers - Marker layers.
 * @param {*} props.x - Marker x coordinate.
 * @param {*} props.y - Marker y coordinate.
 * @returns {JSX.Element|null} Rendered marker.
 */
function PreviewMarkerLayers({ layers, x, y }) {
  if (!layers?.length || !Number.isFinite(x) || !Number.isFinite(y)) {
    return null
  }

  return (
    <>
      {layers.map((layer, index) => (
        <circle
          key={`${layer.radius}-${layer.color}-${index}`}
          cx={x}
          cy={y}
          r={layer.radius}
          fill={layer.solidFill ? layer.color : 'none'}
          stroke={layer.solidFill ? 'none' : layer.color}
          strokeWidth={
            layer.solidFill
              ? undefined
              : Math.min(Math.max(Math.round(layer.radius * 0.18), 1), 3)
          }
          opacity={layer.opacity}
        />
      ))}
    </>
  )
}

/**
 * Builds route completed-prefix points like the Skia renderer.
 *
 * @param {*} points - Geometry points.
 * @param {*} progressValues - Geometry progress values.
 * @param {*} progress01 - Current progress.
 * @returns {object} Marker and completed points.
 */
function buildRouteFramePreview(points, progressValues, progress01) {
  if (!points.length) {
    return { markerPoint: null, completedPoints: [] }
  }

  const metricPoint = getPointAtMetricProgressWithIndex(
    points,
    progressValues,
    progress01,
  )
  const markerPoint =
    metricPoint?.point ||
    getPointAtProgress(points, progress01) ||
    points[points.length - 1]
  const lastPoint = points[points.length - 1]
  let completedPoints =
    markerPoint && pointsEqual(lastPoint, markerPoint)
      ? [...points]
      : points.slice(0, Math.min(metricPoint?.index ?? 0, points.length))

  if (!completedPoints.length) {
    completedPoints = [points[0]]
  }

  if (
    markerPoint &&
    !pointsEqual(completedPoints[completedPoints.length - 1], markerPoint)
  ) {
    completedPoints.push(markerPoint)
  }

  return { markerPoint, completedPoints }
}

/**
 * Builds elevation completed points like the Skia renderer.
 *
 * @param {*} points - Geometry points.
 * @param {*} progressValues - Geometry progress values.
 * @param {*} progress01 - Current progress.
 * @param {*} markerPoint - Current marker point.
 * @returns {Array} Completed points.
 */
function buildElevationCompletedPoints(
  points,
  progressValues,
  progress01,
  markerPoint,
) {
  if (!points.length) {
    return []
  }

  const completedPoints = points.filter(
    (_, index) => (progressValues[index] ?? 0) <= progress01,
  )

  if (!completedPoints.length) {
    completedPoints.push(points[0])
  }

  if (
    markerPoint &&
    !pointsEqual(completedPoints[completedPoints.length - 1], markerPoint)
  ) {
    completedPoints.push(markerPoint)
  }

  return completedPoints
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
 * Renders a metric icon inside the preview svg.
 *
 * @param {object} props - Component props.
 * @param {*} props.icon - Parsed icon data.
 * @param {*} props.left - Horizontal coordinate.
 * @param {*} props.top - Vertical coordinate.
 * @param {*} props.size - Numeric icon size value.
 * @param {*} props.color - Stroke color.
 * @param {*} props.opacity - Opacity value.
 * @returns {JSX.Element|null} Rendered component output.
 */
function PreviewMetricIcon({ icon, left, top, size, color, opacity }) {
  if (!icon?.innerMarkup || size <= 0) {
    return null
  }

  return (
    <g
      transform={`translate(${left} ${top}) scale(${size / 24})`}
      fill="none"
      stroke={color}
      strokeWidth={icon.strokeWidth || 2}
      strokeLinecap="round"
      strokeLinejoin="round"
      opacity={opacity}
      dangerouslySetInnerHTML={{ __html: icon.innerMarkup }}
    />
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
  globalScale,
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
            scale: globalScale || 1,
          })
        : null,
    [
      currentGradientValue,
      fontFamily,
      fontSize,
      globalScale,
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

    return (
      <div
        className="relative"
        style={{
          width: metricLayout.width,
          height: metricLayout.height,
        }}
      >
        <div
          className="absolute"
          style={{ width: metricLayout.width, height: metricLayout.height }}
        >
          <svg
            width={metricLayout.width}
            height={metricLayout.height}
            viewBox={`0 0 ${metricLayout.width} ${metricLayout.height}`}
            className="absolute left-0 top-0 block overflow-visible"
          >
            {metricLayout.icon && iconSvg ? (
              <PreviewMetricIcon
                icon={iconSvg}
                left={iconLeft}
                top={iconTop}
                size={metricLayout.icon.size}
                color={widget.data.icon_color || '#40e0d0'}
                opacity={widgetOpacity}
              />
            ) : null}
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
  globalScale,
}) {
  const width = Math.max(widget.data.width ?? 320, 80)
  const height = Math.max(widget.data.height ?? 180, 80)
  const safeGlobalScale = Math.max(Number(globalScale) || 1, 0.1)
  const baseColor = widget.data.color || '#ffffff'
  const geometryRemainingLineWidth = resolvePreviewLineWidth(
    widget.data.remaining_line_width,
    widget.data.line?.width,
  )
  const geometryCompletedLineWidth = resolvePreviewLineWidth(
    widget.data.completed_line_width,
    widget.data.line?.width,
  )
  const remainingLineWidth = resolveScaledPreviewLineWidth(
    widget.data.remaining_line_width,
    widget.data.line?.width,
    safeGlobalScale,
  )
  const completedLineWidth = resolveScaledPreviewLineWidth(
    widget.data.completed_line_width,
    widget.data.line?.width,
    safeGlobalScale,
  )
  const remainingLineColor = resolvePreviewStyleColor(
    widget.data.remaining_line_color,
    widget.data.line?.color,
    baseColor,
  )
  const completedLineColor = resolvePreviewStyleColor(
    widget.data.completed_line_color,
    widget.data.line?.color,
    baseColor,
  )
  const remainingLineOpacity = normalizePreviewOpacity(
    widget.data.remaining_line_opacity ??
      widget.data.line?.opacity ??
      widget.data.opacity,
    0.75,
  )
  const completedLineOpacity = normalizePreviewOpacity(
    widget.data.completed_line_opacity ??
      widget.data.line?.opacity ??
      widget.data.opacity,
    1,
  )
  const markerSize = Number.isFinite(Number(widget.data.marker_size))
    ? Number(widget.data.marker_size)
    : 18
  const svgMarkerSize = markerSize / safeGlobalScale
  const markerColor = widget.data.marker_color || baseColor
  const markerOpacity = normalizePreviewOpacity(
    widget.data.marker_opacity ?? widget.data.opacity,
    1,
  )
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
        geometryRemainingLineWidth,
        geometryCompletedLineWidth,
        markerSize,
      ),
    [
      geometryCompletedLineWidth,
      geometryRemainingLineWidth,
      height,
      markerSize,
      routeSamples,
      width,
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
  const { markerPoint, completedPoints } = useMemo(
    () =>
      buildRouteFramePreview(routeGeometry.points, pointProgress, progress01),
    [pointProgress, progress01, routeGeometry.points],
  )
  const remainingSvgPoints = useMemo(
    () => pointsToSvg(routeGeometry.points),
    [routeGeometry.points],
  )
  const completedSvgPoints = useMemo(
    () => pointsToSvg(completedPoints),
    [completedPoints],
  )
  const markerLayers = useMemo(
    () =>
      getPreviewMarkerLayers(
        widget.data,
        svgMarkerSize,
        markerColor,
        markerOpacity,
      ),
    [markerColor, markerOpacity, svgMarkerSize, widget.data],
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
          stroke={remainingLineColor}
          strokeOpacity={remainingLineOpacity}
          strokeWidth={remainingLineWidth}
          strokeLinejoin="round"
          strokeLinecap="round"
          points={remainingSvgPoints}
        />
        <polyline
          fill="none"
          stroke={completedLineColor}
          strokeOpacity={completedLineOpacity}
          strokeWidth={completedLineWidth}
          strokeLinejoin="round"
          strokeLinecap="round"
          points={completedSvgPoints}
        />
        <PreviewMarkerLayers
          layers={markerLayers}
          x={markerPoint?.[0]}
          y={markerPoint?.[1]}
        />
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
  globalScale,
  sceneFont,
  sceneFontSize,
}) {
  const width = Math.max(widget.data.width ?? 320, 80)
  const height = Math.max(widget.data.height ?? 180, 80)
  const safeGlobalScale = Math.max(Number(globalScale) || 1, 0.1)
  const baseColor = widget.data.color || '#ffffff'
  const remainingLineWidth = resolveScaledPreviewLineWidth(
    widget.data.remaining_line_width,
    widget.data.line?.width,
    safeGlobalScale,
  )
  const completedLineWidth = resolveScaledPreviewLineWidth(
    widget.data.completed_line_width,
    widget.data.line?.width,
    safeGlobalScale,
  )
  const remainingLineColor = resolvePreviewStyleColor(
    widget.data.remaining_line_color,
    widget.data.line?.color,
    baseColor,
  )
  const completedLineColor = resolvePreviewStyleColor(
    widget.data.completed_line_color,
    widget.data.line?.color,
    baseColor,
  )
  const remainingLineOpacity = normalizePreviewOpacity(
    widget.data.remaining_line_opacity ??
      widget.data.line?.opacity ??
      widget.data.opacity,
    1,
  )
  const completedLineOpacity = normalizePreviewOpacity(
    widget.data.completed_line_opacity ??
      widget.data.line?.opacity ??
      widget.data.opacity,
    1,
  )
  const markerSize = Number.isFinite(Number(widget.data.marker_size))
    ? Number(widget.data.marker_size)
    : 16
  const svgMarkerSize = markerSize / safeGlobalScale
  const markerColor = widget.data.marker_color || baseColor
  const markerOpacity = normalizePreviewOpacity(
    widget.data.marker_opacity ?? widget.data.opacity,
    1,
  )
  const labelFontSize =
    widget.data.point_label?.font_size ?? sceneFontSize ?? 12.5
  const labelFontFamily = getPreviewFontFamily(
    widget.data.point_label?.font ||
      widget.data.point_label?.font_family ||
      sceneFont ||
      widget.data.font ||
      widget.data.font_family,
  )
  const labelFontMetricsVersion = useFontMetricsVersion(
    labelFontFamily,
    labelFontSize,
  )
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
    widget.data.area_remaining_color || widget.data.fill?.color || baseColor
  const completedAreaColor =
    widget.data.area_completed_color || widget.data.fill?.color || baseColor
  const remainingAreaOpacity = normalizePreviewOpacity(
    widget.data.area_remaining_opacity ??
      (widget.data.fill?.opacity === undefined
        ? undefined
        : widget.data.fill.opacity * 0.35),
    0.12,
  )
  const completedAreaOpacity = normalizePreviewOpacity(
    widget.data.area_completed_opacity ?? widget.data.fill?.opacity,
    0.24,
  )
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
        widget.data.margin ?? 0,
        widget.data.y_scale ?? 1,
        profileDistanceProgress,
        widget.data.target_density ?? 0.75,
        widget.data.simplify_tolerance_px ?? 1,
      ),
    [
      height,
      profileDistanceProgress,
      profileElevations,
      width,
      widget.data.margin,
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
  const completedPoints = useMemo(
    () =>
      buildElevationCompletedPoints(
        points,
        pointProgress,
        progress01,
        markerPoint,
      ),
    [markerPoint, pointProgress, points, progress01],
  )
  const elevationValue =
    getInterpolatedSeriesValue(
      profileDistanceProgress,
      profileElevations,
      progress01,
    ) ?? getSeriesValueAtProgress(profileElevations, progress01)
  const areaSvgPoints = useMemo(
    () => areaToSvg(points, width, height, null),
    [height, points, width],
  )
  const completedAreaSvgPoints = useMemo(
    () => areaToSvg(completedPoints, width, height, null),
    [completedPoints, height, width],
  )
  const remainingSvgPoints = pointsToSvg(points)
  const completedSvgPoints = pointsToSvg(completedPoints)
  const metricLabel =
    elevationValue === null || elevationValue === undefined
      ? '-- m'
      : `${Math.round(elevationValue)} m`
  const imperialLabel =
    elevationValue === null || elevationValue === undefined
      ? '-- ft'
      : `${Math.round(elevationValue * 3.28084)} ft`
  const markerLayers = useMemo(
    () =>
      getPreviewMarkerLayers(
        widget.data,
        svgMarkerSize,
        markerColor,
        markerOpacity,
      ),
    [markerColor, markerOpacity, svgMarkerSize, widget.data],
  )
  const labelMeasurement = useMemo(
    () => measurePreviewText(metricLabel, labelFontSize, labelFontFamily),
    [labelFontFamily, labelFontMetricsVersion, labelFontSize, metricLabel],
  )
  const getElevationLabelBaseline = (top) =>
    getPreviewTextBaseline({
      top,
      lineHeight: labelFontSize * 0.92,
      ascent: labelMeasurement.ascent,
      descent: labelMeasurement.descent,
      glyphHeight: labelMeasurement.glyphHeight,
    })
  const labelColor = widget.data.point_label?.color || baseColor
  const showMetricLabel = widget.data.show_elevation_metric ?? false
  const showImperialLabel = widget.data.show_elevation_imperial ?? false

  return (
    <svg
      width={width}
      height={height}
      viewBox={`0 0 ${width} ${height}`}
      className="block h-full w-full overflow-visible"
      style={{ opacity: getWidgetOpacity(widget.data, globalOpacity) }}
    >
      <polygon
        points={areaSvgPoints}
        fill={remainingAreaColor}
        fillOpacity={remainingAreaOpacity}
      />
      <polyline
        fill="none"
        stroke={remainingLineColor}
        strokeOpacity={remainingLineOpacity}
        strokeWidth={remainingLineWidth}
        strokeLinejoin="round"
        strokeLinecap="round"
        points={remainingSvgPoints}
      />
      <polygon
        points={completedAreaSvgPoints}
        fill={completedAreaColor}
        fillOpacity={completedAreaOpacity}
      />
      <polyline
        fill="none"
        stroke={completedLineColor}
        strokeOpacity={completedLineOpacity}
        strokeWidth={completedLineWidth}
        strokeLinejoin="round"
        strokeLinecap="round"
        points={completedSvgPoints}
      />
      <PreviewMarkerLayers
        layers={markerLayers}
        x={markerPoint?.[0]}
        y={markerPoint?.[1]}
      />
      {markerPoint && showMetricLabel ? (
        <text
          x={markerPoint[0] + (widget.data.metric_label_offset_x ?? 0)}
          y={getElevationLabelBaseline(
            markerPoint[1] + (widget.data.metric_label_offset_y ?? -28),
          )}
          fill={labelColor}
          fontFamily={labelFontFamily}
          fontSize={labelFontSize}
        >
          {metricLabel}
        </text>
      ) : null}
      {markerPoint && showImperialLabel ? (
        <text
          x={markerPoint[0] + (widget.data.imperial_label_offset_x ?? 0)}
          y={getElevationLabelBaseline(
            markerPoint[1] + (widget.data.imperial_label_offset_y ?? 6),
          )}
          fill={labelColor}
          fontFamily={labelFontFamily}
          fontSize={labelFontSize}
        >
          {imperialLabel}
        </text>
      ) : null}
    </svg>
  )
}
