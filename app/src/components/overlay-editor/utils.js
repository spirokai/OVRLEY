import { DEFAULT_ACTIVITY_PREVIEW, FONT_FAMILY_MAP } from './constants'

export const METRIC_WIDGET_LINE_HEIGHT = 0.92
export const METRIC_WIDGET_OUTER_GAP_PX = 8
export const METRIC_WIDGET_UNITS_GAP_PX = 8

export function clamp(value, min, max) {
  return Math.min(max, Math.max(min, value))
}

export function getElapsedSeries(activity) {
  const frameElapsedSeries = activity?.frame_elapsed_seconds
  if (Array.isArray(frameElapsedSeries) && frameElapsedSeries.length) {
    return frameElapsedSeries
  }

  return Array.isArray(activity?.sample_elapsed_seconds)
    ? activity.sample_elapsed_seconds
    : []
}

export function getSceneSize(config) {
  return {
    width: config?.scene?.width || 1920,
    height: config?.scene?.height || 1080,
  }
}

export function getPreviewFontFamily(fontName) {
  return FONT_FAMILY_MAP[fontName] || FONT_FAMILY_MAP['Arial.ttf']
}

let metricMeasureContext = null

function getMetricMeasureContext() {
  if (metricMeasureContext) {
    return metricMeasureContext
  }

  const canvas = document.createElement('canvas')
  metricMeasureContext = canvas.getContext('2d')
  return metricMeasureContext
}

export function measurePreviewText(text, fontSize, fontFamily) {
  if (!text) {
    return {
      width: 0,
      glyphHeight: 0,
      ascent: 0,
      descent: 0,
      boundsLeft: 0,
      boundsRight: 0,
    }
  }

  const context = getMetricMeasureContext()
  if (!context) {
    return {
      width: 0,
      glyphHeight: 0,
      ascent: 0,
      descent: 0,
      boundsLeft: 0,
      boundsRight: 0,
    }
  }

  context.font = `${fontSize}px ${fontFamily}`
  const metrics = context.measureText(text)
  const ascent = metrics.actualBoundingBoxAscent || 0
  const descent = metrics.actualBoundingBoxDescent || 0
  const glyphHeight = ascent + descent

  return {
    width: metrics.width,
    glyphHeight,
    ascent,
    descent,
    boundsLeft: metrics.actualBoundingBoxLeft || 0,
    boundsRight: metrics.actualBoundingBoxRight || metrics.width,
  }
}

export function getPreviewTextBaseline({
  top = 0,
  lineHeight,
  ascent,
  descent: _descent,
  glyphHeight,
}) {
  if (!glyphHeight) {
    return top + lineHeight
  }

  return top + ((lineHeight - glyphHeight) / 2 + ascent)
}

export function getMetricWidgetLayout({
  fontSize,
  fontFamily,
  valueText,
  unitText,
  showIcon,
  showUnits,
  iconSize,
}) {
  const valueLineHeight = fontSize * METRIC_WIDGET_LINE_HEIGHT
  const unitsFontSize = Math.max(fontSize * 0.28, 12)
  const unitsLineHeight = unitsFontSize * METRIC_WIDGET_LINE_HEIGHT
  const iconMarginRight = Math.max(fontSize * 0.08, 8)
  const valueMeasure = measurePreviewText(valueText, fontSize, fontFamily)
  const showUnitText = Boolean(showUnits && unitText)
  const unitsMeasure = showUnitText
    ? measurePreviewText(unitText, unitsFontSize, fontFamily)
    : {
        width: 0,
        glyphHeight: 0,
        ascent: 0,
        descent: 0,
        boundsLeft: 0,
        boundsRight: 0,
      }
  const textGroupHeight = showUnitText
    ? Math.max(valueLineHeight, unitsLineHeight)
    : valueLineHeight
  const rowHeight = Math.max(showIcon ? iconSize : 0, textGroupHeight)
  const textGroupLeft = showIcon
    ? iconSize + METRIC_WIDGET_OUTER_GAP_PX + iconMarginRight
    : 0
  const textGroupTop = (rowHeight - textGroupHeight) / 2
  const textGroupBottom = textGroupTop + textGroupHeight
  const valueTop =
    textGroupBottom - (valueLineHeight + valueMeasure.glyphHeight) / 2
  const unitsTop =
    textGroupBottom - (unitsLineHeight + unitsMeasure.glyphHeight) / 2
  const unitsLeft =
    textGroupLeft + valueMeasure.width + METRIC_WIDGET_UNITS_GAP_PX
  const width = showUnitText
    ? unitsLeft + unitsMeasure.width
    : textGroupLeft + valueMeasure.width

  return {
    icon: showIcon
      ? {
          left: 0,
          top: (rowHeight - iconSize) / 2,
          size: iconSize,
        }
      : null,
    value: {
      left: textGroupLeft,
      top: valueTop,
      baseline: getPreviewTextBaseline({
        top: valueTop,
        lineHeight: valueLineHeight,
        ascent: valueMeasure.ascent,
        descent: valueMeasure.descent,
        glyphHeight: valueMeasure.glyphHeight,
      }),
      width: valueMeasure.width,
      lineHeight: valueLineHeight,
    },
    units: showUnitText
      ? {
          left: unitsLeft,
          top: unitsTop,
          baseline: getPreviewTextBaseline({
            top: unitsTop,
            lineHeight: unitsLineHeight,
            ascent: unitsMeasure.ascent,
            descent: unitsMeasure.descent,
            glyphHeight: unitsMeasure.glyphHeight,
          }),
          width: unitsMeasure.width,
          fontSize: unitsFontSize,
          lineHeight: unitsLineHeight,
        }
      : null,
    width,
    height: rowHeight,
    unitsFontSize,
  }
}

export function getWidgetOpacity(data, globalOpacity = 1) {
  return clamp((data?.opacity ?? 1) * globalOpacity, 0, 1)
}

export function getTextShadow(data) {
  const shadowStrength = Number(data?.shadow_strength) || 0
  const shadowDistance = Number(data?.shadow_distance) || 0
  const shadowColor = data?.shadow_color

  if (!shadowStrength || !shadowColor) return undefined

  return `${shadowDistance}px ${shadowDistance}px ${shadowStrength}px ${shadowColor}`
}

export function getTextOutlineShadow(data) {
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

export function getCombinedTextShadow(data) {
  const outlineShadow = getTextOutlineShadow(data)
  const dropShadow = getTextShadow(data)

  if (outlineShadow && dropShadow) {
    return `${outlineShadow}, ${dropShadow}`
  }

  return outlineShadow || dropShadow || undefined
}

export function findClosestSampleIndex(activity, selectedSecond) {
  const elapsedSeries = getElapsedSeries(activity)
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

export function getSampleValue(activity, key, sampleIndex) {
  const series = activity?.[key]
  if (!Array.isArray(series)) {
    return DEFAULT_ACTIVITY_PREVIEW[key] ?? null
  }

  return series[sampleIndex] ?? DEFAULT_ACTIVITY_PREVIEW[key] ?? null
}

export function getInterpolatedSeriesValue(xValues, yValues, targetX) {
  if (!Array.isArray(xValues) || !Array.isArray(yValues) || !xValues.length) {
    return null
  }

  const safeTargetX = Number(targetX)
  if (!Number.isFinite(safeTargetX)) {
    return null
  }

  let firstValidIndex = -1
  let lastValidIndex = -1

  for (let index = 0; index < xValues.length; index += 1) {
    if (Number.isFinite(xValues[index]) && Number.isFinite(yValues[index])) {
      firstValidIndex = index
      break
    }
  }

  if (firstValidIndex === -1) {
    return null
  }

  for (let index = yValues.length - 1; index >= 0; index -= 1) {
    if (Number.isFinite(xValues[index]) && Number.isFinite(yValues[index])) {
      lastValidIndex = index
      break
    }
  }

  if (safeTargetX <= xValues[firstValidIndex]) {
    return Number(yValues[firstValidIndex])
  }

  if (safeTargetX >= xValues[lastValidIndex]) {
    return Number(yValues[lastValidIndex])
  }

  let leftIndex = firstValidIndex
  let rightIndex = firstValidIndex

  for (let index = firstValidIndex + 1; index <= lastValidIndex; index += 1) {
    const nextX = Number(xValues[index])
    const nextY = Number(yValues[index])
    if (!Number.isFinite(nextX) || !Number.isFinite(nextY)) {
      continue
    }

    if (nextX >= safeTargetX) {
      rightIndex = index
      break
    }

    leftIndex = index
  }

  const leftX = Number(xValues[leftIndex])
  const rightX = Number(xValues[rightIndex])
  const leftY = Number(yValues[leftIndex])
  const rightY = Number(yValues[rightIndex])

  if (
    !Number.isFinite(leftX) ||
    !Number.isFinite(rightX) ||
    !Number.isFinite(leftY) ||
    !Number.isFinite(rightY)
  ) {
    return null
  }

  if (rightIndex === leftIndex || rightX === leftX) {
    return leftY
  }

  const ratio = (safeTargetX - leftX) / (rightX - leftX)
  return leftY + (rightY - leftY) * ratio
}

export function getInterpolatedActivityValue(activity, key, elapsedSecond) {
  const elapsedSeries = Array.isArray(activity?.sample_elapsed_seconds)
    ? activity.sample_elapsed_seconds
    : []
  const series = activity?.[key]

  if (!Array.isArray(series) || !elapsedSeries.length) {
    return DEFAULT_ACTIVITY_PREVIEW[key] ?? null
  }

  const interpolatedValue = getInterpolatedSeriesValue(
    elapsedSeries,
    series,
    elapsedSecond,
  )

  return interpolatedValue ?? DEFAULT_ACTIVITY_PREVIEW[key] ?? null
}

export function getInterpolatedTimeValue(activity, elapsedSecond) {
  const sourceStartTimeMs = Date.parse(activity?.source_start_time || '')
  if (Number.isFinite(sourceStartTimeMs)) {
    return new Date(
      sourceStartTimeMs + Math.max(elapsedSecond, 0) * 1000,
    ).toISOString()
  }

  const elapsedSeries = Array.isArray(activity?.sample_elapsed_seconds)
    ? activity.sample_elapsed_seconds
    : []
  const timeSeries = Array.isArray(activity?.time) ? activity.time : []
  const numericTimeSeries = timeSeries.map((value) => {
    const parsed = Date.parse(value || '')
    return Number.isFinite(parsed) ? parsed : null
  })
  const interpolatedTimeMs = getInterpolatedSeriesValue(
    elapsedSeries,
    numericTimeSeries,
    elapsedSecond,
  )

  return Number.isFinite(interpolatedTimeMs)
    ? new Date(interpolatedTimeMs).toISOString()
    : DEFAULT_ACTIVITY_PREVIEW.time
}

export function getDistanceProgress(activity, sampleIndex) {
  const distanceProgressSeries =
    activity?.frame_distance_progress?.length > 0
      ? activity.frame_distance_progress
      : activity?.sample_distance_progress?.length > 0
        ? activity.sample_distance_progress
        : null

  if (distanceProgressSeries) {
    const progressValue =
      distanceProgressSeries[
        clamp(sampleIndex, 0, distanceProgressSeries.length - 1)
      ]

    return clamp(Number(progressValue) || 0, 0, 1)
  }

  const elapsedSeries = getElapsedSeries(activity)
  if (elapsedSeries.length <= 1) {
    return 0
  }

  return clamp(sampleIndex / (elapsedSeries.length - 1), 0, 1)
}

export function getDistanceProgressAtElapsed(activity, elapsedSecond) {
  const elapsedSeries = Array.isArray(activity?.sample_elapsed_seconds)
    ? activity.sample_elapsed_seconds
    : []
  const distanceProgressSeries = Array.isArray(
    activity?.sample_distance_progress,
  )
    ? activity.sample_distance_progress
    : []

  const interpolatedProgress = getInterpolatedSeriesValue(
    elapsedSeries,
    distanceProgressSeries,
    elapsedSecond,
  )

  if (Number.isFinite(interpolatedProgress)) {
    return clamp(interpolatedProgress, 0, 1)
  }

  if (elapsedSeries.length <= 1) {
    return 0
  }

  const safeElapsed = clamp(
    Number(elapsedSecond) || 0,
    elapsedSeries[0] ?? 0,
    elapsedSeries[elapsedSeries.length - 1] ?? 0,
  )
  const totalElapsed =
    (elapsedSeries[elapsedSeries.length - 1] ?? 0) - (elapsedSeries[0] ?? 0)

  if (totalElapsed <= 0) {
    return 0
  }

  return clamp((safeElapsed - (elapsedSeries[0] ?? 0)) / totalElapsed, 0, 1)
}

export function formatSpeed(value, unit) {
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

export function formatTemperature(value, unit) {
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

export function formatTimeValue(format, timestamp) {
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

export function formatGradientValue(widget, value) {
  if (value === null || value === undefined) return '--'

  const decimals = widget.data.decimals ?? 0
  const numericValue = Number(value)
  const absoluteValue = Math.abs(numericValue).toFixed(decimals)
  const sign = numericValue > 0 ? '+' : numericValue < 0 ? '-' : ''
  const prefix = widget.data.show_sign === false ? '' : sign

  return `${prefix}${absoluteValue}`
}

export function buildGradientTrianglePath(value, width, height) {
  const normalized = clamp(Math.abs(Number(value) || 0) / 15, 0.12, 1)
  const centeredHeight = Math.max(height * 0.88, 4)
  const rise = Math.max((centeredHeight / 2) * normalized, 2)
  const centerY = height / 2

  if (Number(value) >= 0) {
    return `M 0 ${centerY} L ${width} ${centerY} L ${width} ${centerY - rise} Z`
  }

  return `M 0 ${centerY} L ${width} ${centerY} L ${width} ${centerY + rise} Z`
}

export function getSeriesValueAtProgress(series, progress01) {
  if (!Array.isArray(series) || !series.length) {
    return null
  }

  const clampedProgress = clamp(Number(progress01) || 0, 0, 1)
  const scaledIndex = clampedProgress * (series.length - 1)
  const startIndex = Math.floor(scaledIndex)
  const endIndex = Math.min(startIndex + 1, series.length - 1)
  const mix = scaledIndex - startIndex
  const startValue = Number(series[startIndex])
  const endValue = Number(series[endIndex])

  if (!Number.isFinite(startValue) && !Number.isFinite(endValue)) {
    return null
  }

  if (!Number.isFinite(startValue)) {
    return endValue
  }

  if (!Number.isFinite(endValue)) {
    return startValue
  }

  return startValue + (endValue - startValue) * mix
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

function fitPointsToWidget(points, width, height, insetPx, invertY = true) {
  if (!points.length) {
    return []
  }

  const minX = Math.min(...points.map(([x]) => x))
  const maxX = Math.max(...points.map(([x]) => x))
  const minY = Math.min(...points.map(([, y]) => y))
  const maxY = Math.max(...points.map(([, y]) => y))
  const safeInset = Math.min(
    Math.max(Number(insetPx) || 0, 0),
    Math.min(width, height) * 0.45,
  )
  const innerWidth = Math.max(width - safeInset * 2, 1)
  const innerHeight = Math.max(height - safeInset * 2, 1)
  const spanX = Math.max(maxX - minX, 0.000001)
  const spanY = Math.max(maxY - minY, 0.000001)
  const scale = Math.min(innerWidth / spanX, innerHeight / spanY)
  const offsetX = (width - spanX * scale) / 2
  const offsetY = (height - spanY * scale) / 2

  return points.map(([x, y]) => {
    const fittedX = (x - minX) * scale + offsetX
    let fittedY = (y - minY) * scale + offsetY
    if (invertY) {
      fittedY = height - fittedY
    }
    return [fittedX, fittedY]
  })
}

function routeGeometryInsetPx(
  widgetWidth,
  widgetHeight,
  lineWidth,
  completedLineWidth,
  markerSize,
) {
  const safeWidth = Number(lineWidth) || 0
  const safeCompletedWidth = Number(completedLineWidth) || 0
  const safeMarkerSize = Number(markerSize) || 0
  const lineInset = Math.max(safeWidth, safeCompletedWidth) * 0.5
  return Math.min(
    Math.max(safeMarkerSize, lineInset) + 1,
    Math.min(widgetWidth, widgetHeight) * 0.45,
  )
}

function simplifyRouteSamples(samples, tolerance) {
  if (samples.length <= 2 || tolerance <= 0) {
    return samples
  }

  const perpendicularDistance = (point, start, end) => {
    const [x0, y0] = point.point
    const [x1, y1] = start.point
    const [x2, y2] = end.point
    const dx = x2 - x1
    const dy = y2 - y1
    if (Math.abs(dx) <= Number.EPSILON && Math.abs(dy) <= Number.EPSILON) {
      return Math.hypot(x0 - x1, y0 - y1)
    }
    return Math.abs(dy * x0 - dx * y0 + x2 * y1 - y2 * x1) / Math.hypot(dx, dy)
  }

  let maxDistance = 0
  let splitIndex = 0
  for (let index = 1; index < samples.length - 1; index += 1) {
    const distance = perpendicularDistance(
      samples[index],
      samples[0],
      samples[samples.length - 1],
    )
    if (distance > maxDistance) {
      maxDistance = distance
      splitIndex = index
    }
  }

  if (maxDistance <= tolerance) {
    return [samples[0], samples[samples.length - 1]]
  }

  const left = simplifyRouteSamples(samples.slice(0, splitIndex + 1), tolerance)
  const right = simplifyRouteSamples(samples.slice(splitIndex), tolerance)
  return [...left.slice(0, -1), ...right]
}

export function normalizeRouteGeometry(
  samples,
  width,
  height,
  targetDensity = 1,
  simplifyTolerancePx = 1,
  lineWidth = 6,
  completedLineWidth = 6,
  markerSize = 18,
) {
  const validSamples = samples.filter(
    (sample) =>
      Array.isArray(sample?.point) &&
      Number.isFinite(sample.point[0]) &&
      Number.isFinite(sample.point[1]),
  )

  if (validSamples.length < 2) {
    const fallbackPoints = buildFallbackRoute(width, height)
    return {
      points: fallbackPoints,
      progressValues: fallbackPoints.map((_, index) =>
        fallbackPoints.length > 1 ? index / (fallbackPoints.length - 1) : 0,
      ),
    }
  }

  const validPoints = validSamples.map((sample) => sample.point)
  const latitudes = validPoints.map(([latitude]) => latitude)
  const meanLatitude =
    latitudes.reduce((sum, latitude) => sum + latitude, 0) / latitudes.length
  const meanLatitudeRadians = meanLatitude * (Math.PI / 180)
  const projectedPoints = validPoints.map(([latitude, longitude]) => [
    longitude * Math.cos(meanLatitudeRadians),
    latitude,
  ])
  const fitted = fitPointsToWidget(
    projectedPoints,
    width,
    height,
    routeGeometryInsetPx(
      width,
      height,
      lineWidth,
      completedLineWidth,
      markerSize,
    ),
    true,
  )
  const fittedSamples = validSamples.map((sample, index) => ({
    point: fitted[index],
    progress: Number.isFinite(sample.progress)
      ? clamp(sample.progress, 0, 1)
      : 0,
  }))
  const safeTargetDensity = clamp(Number(targetDensity) || 1, 0.1, 2)
  const targetCount = Math.max(
    2,
    Math.min(fittedSamples.length, Math.round(width * safeTargetDensity)),
  )
  const downsampled = downsampleRouteSamples(fittedSamples, targetCount)
  const simplified = simplifyRouteSamples(
    downsampled,
    Math.max(clamp(Number(simplifyTolerancePx) || 1, 0, 8), 0.05),
  )

  return {
    points: simplified.map((sample) => sample.point),
    progressValues: simplified.map((sample) => sample.progress),
  }
}

function downsampleRouteSamples(samples, targetCount) {
  if (samples.length <= targetCount || targetCount < 3) {
    return samples
  }

  const bucketSize = (samples.length - 2) / (targetCount - 2)
  const sampled = [samples[0]]
  let a = 0

  for (let bucketIndex = 0; bucketIndex < targetCount - 2; bucketIndex += 1) {
    const avgStart = Math.floor((bucketIndex + 1) * bucketSize) + 1
    const avgEnd = Math.min(
      samples.length,
      Math.floor((bucketIndex + 2) * bucketSize) + 1,
    )
    const avgRangeStart = Math.min(avgStart, Math.max(avgEnd - 1, 0))
    const avgRange = samples.slice(avgRangeStart, avgEnd)
    const average =
      avgRange.length > 0
        ? {
            x:
              avgRange.reduce((sum, sample) => sum + sample.point[0], 0) /
              avgRange.length,
            y:
              avgRange.reduce((sum, sample) => sum + sample.point[1], 0) /
              avgRange.length,
          }
        : {
            x: samples[samples.length - 1].point[0],
            y: samples[samples.length - 1].point[1],
          }

    const rangeStart = Math.floor(bucketIndex * bucketSize) + 1
    const rangeEnd = Math.min(
      samples.length - 1,
      Math.floor((bucketIndex + 1) * bucketSize) + 1,
    )
    const candidateStart = Math.min(rangeStart, samples.length - 2)
    const candidateEnd = Math.max(candidateStart + 1, rangeEnd)

    let nextA = candidateStart
    let maxArea = -1
    for (
      let candidateIndex = candidateStart;
      candidateIndex < candidateEnd;
      candidateIndex += 1
    ) {
      const pointA = samples[a].point
      const pointB = samples[candidateIndex].point
      const area =
        Math.abs(
          (pointA[0] - average.x) * (pointB[1] - pointA[1]) -
            (pointA[0] - pointB[0]) * (average.y - pointA[1]),
        ) * 0.5
      if (area > maxArea) {
        maxArea = area
        nextA = candidateIndex
      }
    }

    a = nextA
    sampled.push(samples[a])
  }

  sampled.push(samples[samples.length - 1])
  return sampled
}

export function normalizeRoutePoints(points, width, height, _padding = 18) {
  return normalizeRouteGeometry(
    points.map((point, index) => ({
      point,
      progress: points.length > 1 ? index / (points.length - 1) : 0,
    })),
    width,
    height,
    1,
    1,
    6,
    6,
    18,
  ).points
}

export function buildWidgetTransform({ scale = 1, rotation = 0 }) {
  const transforms = []

  if (rotation) {
    transforms.push(`rotate(${rotation}deg)`)
  }

  if (scale !== 1) {
    transforms.push(`scale(${scale})`)
  }

  return transforms.length ? transforms.join(' ') : undefined
}

export function normalizeElevationPoints(
  values,
  width,
  height,
  padding = 18,
  verticalScale = 1,
  progressValues = [],
  targetDensity = 0.75,
  simplifyTolerancePx = 1,
) {
  const samples = values.reduce((result, value, index) => {
    if (!Number.isFinite(value)) {
      return result
    }

    const progressValue = Number(progressValues[index])
    result.push({
      progress: Number.isFinite(progressValue)
        ? clamp(progressValue, 0, 1)
        : null,
      value: Number(value),
    })
    return result
  }, [])

  if (!samples.length) {
    return [
      [padding, height - padding],
      [width * 0.32, height * 0.55],
      [width * 0.62, height * 0.36],
      [width - padding, height * 0.48],
    ]
  }

  const usableValues = samples.map((sample) => sample.value)
  const minimum = Math.min(...usableValues)
  const maximum = Math.max(...usableValues)
  const amplitude = Math.max(maximum - minimum, 1)
  const usableWidth = Math.max(width - padding * 2, 1)
  const hasUsableProgress = samples.some((sample) =>
    Number.isFinite(sample.progress),
  )
  const safeVerticalScale = clamp(Number(verticalScale) || 1, 0.2, 4)
  const safeTargetDensity = clamp(Number(targetDensity) || 0.75, 0.1, 2)
  const safeSimplifyTolerance = clamp(Number(simplifyTolerancePx) || 0, 0, 8)

  const smoothElevationSamples = (inputSamples) => {
    const coefficients = [-36, 9, 44, 69, 84, 89, 84, 69, 44, 9, -36]
    const radius = Math.floor(coefficients.length / 2)

    return inputSamples.map((sample, index) => {
      let total = 0
      let coefficientTotal = 0

      for (let offset = -radius; offset <= radius; offset += 1) {
        const neighborIndex = index + offset
        if (neighborIndex < 0 || neighborIndex >= inputSamples.length) {
          continue
        }

        const neighborValue = inputSamples[neighborIndex].value
        if (!Number.isFinite(neighborValue)) {
          continue
        }

        const coefficient = coefficients[offset + radius]
        total += neighborValue * coefficient
        coefficientTotal += coefficient
      }

      return {
        ...sample,
        value:
          Math.abs(coefficientTotal) <= Number.EPSILON
            ? sample.value
            : total / coefficientTotal,
        preserve: index === 0 || index === inputSamples.length - 1,
      }
    })
  }

  const downsampleElevationSamples = (inputSamples, targetCount) => {
    if (inputSamples.length <= targetCount || targetCount < 3) {
      return inputSamples.map((sample, index) => ({
        ...sample,
        preserve: index === 0 || index === inputSamples.length - 1,
      }))
    }

    const smoothedSamples = smoothElevationSamples(inputSamples)
    const lastIndex = smoothedSamples.length - 1
    const selectedSamples = []

    for (let sampleIndex = 0; sampleIndex < targetCount; sampleIndex += 1) {
      const sourceIndex = Math.round(
        (sampleIndex * lastIndex) / Math.max(targetCount - 1, 1),
      )
      const nextSample = smoothedSamples[Math.min(sourceIndex, lastIndex)]
      if (
        selectedSamples.length > 0 &&
        selectedSamples[selectedSamples.length - 1].progress ===
          nextSample.progress
      ) {
        continue
      }
      selectedSamples.push(nextSample)
    }

    if (selectedSamples.length === 1 && smoothedSamples.length > 1) {
      selectedSamples.push(smoothedSamples[smoothedSamples.length - 1])
    }

    return selectedSamples
  }

  const simplifyProjectedPoints = (inputPoints, tolerance) => {
    if (inputPoints.length <= 2 || tolerance <= 0) {
      return inputPoints
    }

    const preservedIndexes = inputPoints.reduce((result, point, index) => {
      if (point.preserve) result.push(index)
      return result
    }, [])
    if (preservedIndexes.length >= 2) {
      const result = []
      for (
        let windowIndex = 0;
        windowIndex < preservedIndexes.length - 1;
        windowIndex += 1
      ) {
        const start = preservedIndexes[windowIndex]
        const end = preservedIndexes[windowIndex + 1]
        const simplifiedSegment = simplifyProjectedPointsSegment(
          inputPoints.slice(start, end + 1),
          tolerance,
        )
        if (result.length === 0) {
          result.push(...simplifiedSegment)
        } else {
          result.push(...simplifiedSegment.slice(1))
        }
      }
      return result
    }

    return simplifyProjectedPointsSegment(inputPoints, tolerance)
  }

  const simplifyProjectedPointsSegment = (inputPoints, tolerance) => {
    if (inputPoints.length <= 2 || tolerance <= 0) {
      return inputPoints
    }

    const perpendicularDistance = (point, start, end) => {
      const [x0, y0] = point.point
      const [x1, y1] = start.point
      const [x2, y2] = end.point
      const dx = x2 - x1
      const dy = y2 - y1
      if (Math.abs(dx) <= Number.EPSILON && Math.abs(dy) <= Number.EPSILON) {
        return Math.hypot(x0 - x1, y0 - y1)
      }
      return (
        Math.abs(dy * x0 - dx * y0 + x2 * y1 - y2 * x1) / Math.hypot(dx, dy)
      )
    }

    let maxDistance = 0
    let splitIndex = 0
    for (let index = 1; index < inputPoints.length - 1; index += 1) {
      const distance = perpendicularDistance(
        inputPoints[index],
        inputPoints[0],
        inputPoints[inputPoints.length - 1],
      )
      if (distance > maxDistance) {
        maxDistance = distance
        splitIndex = index
      }
    }

    if (maxDistance <= tolerance) {
      return [inputPoints[0], inputPoints[inputPoints.length - 1]]
    }

    const left = simplifyProjectedPointsSegment(
      inputPoints.slice(0, splitIndex + 1),
      tolerance,
    )
    const right = simplifyProjectedPointsSegment(
      inputPoints.slice(splitIndex),
      tolerance,
    )
    return [...left.slice(0, -1), ...right]
  }

  const targetCount = Math.max(
    2,
    Math.min(samples.length, Math.round(width * safeTargetDensity)),
  )
  const downsampledSamples = downsampleElevationSamples(samples, targetCount)
  const downsampledFallbackStep =
    downsampledSamples.length > 1
      ? usableWidth / (downsampledSamples.length - 1)
      : 0
  const projectedPoints = downsampledSamples.map((sample, index) => {
    const x = hasUsableProgress
      ? padding + (sample.progress ?? 0) * usableWidth
      : padding + index * downsampledFallbackStep
    const normalized =
      amplitude <= 0 ? 0.5 : (sample.value - minimum) / amplitude
    const centered = clamp((normalized - 0.5) * safeVerticalScale + 0.5, 0, 1)
    const y = height - padding - centered * (height - padding * 2)
    return {
      point: [x, y],
      progress: sample.progress,
      preserve: sample.preserve === true,
    }
  })

  return simplifyProjectedPoints(projectedPoints, safeSimplifyTolerance).map(
    ({ point }) => point,
  )
}

export function pointsToSvg(points) {
  return points.map(([x, y]) => `${x},${y}`).join(' ')
}

export function getPointAtProgress(points, progress01) {
  if (!points.length) {
    return null
  }

  if (points.length === 1) {
    return points[0]
  }

  const clampedProgress = clamp(Number(progress01) || 0, 0, 1)
  const scaledIndex = clampedProgress * (points.length - 1)
  const startIndex = Math.floor(scaledIndex)
  const endIndex = Math.min(startIndex + 1, points.length - 1)
  const mix = scaledIndex - startIndex
  const startPoint = points[startIndex]
  const endPoint = points[endIndex]

  if (!startPoint || !endPoint) {
    return points[Math.min(startIndex, points.length - 1)] || null
  }

  return [
    startPoint[0] + (endPoint[0] - startPoint[0]) * mix,
    startPoint[1] + (endPoint[1] - startPoint[1]) * mix,
  ]
}

export function getPointAtMetricProgress(
  points,
  progressValues,
  targetProgress,
) {
  if (
    !Array.isArray(points) ||
    !Array.isArray(progressValues) ||
    !points.length
  ) {
    return null
  }

  const safeTargetProgress = clamp(Number(targetProgress) || 0, 0, 1)
  let firstValidIndex = -1
  let lastValidIndex = -1

  for (let index = 0; index < points.length; index += 1) {
    if (
      points[index] &&
      Number.isFinite(points[index][0]) &&
      Number.isFinite(points[index][1]) &&
      Number.isFinite(progressValues[index])
    ) {
      firstValidIndex = index
      break
    }
  }

  if (firstValidIndex === -1) {
    return getPointAtProgress(points, safeTargetProgress)
  }

  for (let index = points.length - 1; index >= 0; index -= 1) {
    if (
      points[index] &&
      Number.isFinite(points[index][0]) &&
      Number.isFinite(points[index][1]) &&
      Number.isFinite(progressValues[index])
    ) {
      lastValidIndex = index
      break
    }
  }

  if (safeTargetProgress <= progressValues[firstValidIndex]) {
    return points[firstValidIndex]
  }

  if (safeTargetProgress >= progressValues[lastValidIndex]) {
    return points[lastValidIndex]
  }

  let leftIndex = firstValidIndex
  let rightIndex = firstValidIndex

  for (let index = firstValidIndex + 1; index <= lastValidIndex; index += 1) {
    const nextProgress = Number(progressValues[index])
    if (!Number.isFinite(nextProgress)) {
      continue
    }

    if (nextProgress >= safeTargetProgress) {
      rightIndex = index
      break
    }

    leftIndex = index
  }

  const leftProgress = Number(progressValues[leftIndex])
  const rightProgress = Number(progressValues[rightIndex])
  const leftPoint = points[leftIndex]
  const rightPoint = points[rightIndex]

  if (
    !Number.isFinite(leftProgress) ||
    !Number.isFinite(rightProgress) ||
    !leftPoint ||
    !rightPoint
  ) {
    return null
  }

  if (rightIndex === leftIndex || rightProgress === leftProgress) {
    return leftPoint
  }

  const ratio =
    (safeTargetProgress - leftProgress) / (rightProgress - leftProgress)

  return [
    leftPoint[0] + (rightPoint[0] - leftPoint[0]) * ratio,
    leftPoint[1] + (rightPoint[1] - leftPoint[1]) * ratio,
  ]
}

export function getPointAtX(points, targetX) {
  if (!points.length) {
    return null
  }

  if (points.length === 1) {
    return points[0]
  }

  const safeTargetX = Number(targetX)
  if (!Number.isFinite(safeTargetX)) {
    return null
  }

  if (safeTargetX <= points[0][0]) {
    return points[0]
  }

  const lastPoint = points[points.length - 1]
  if (safeTargetX >= lastPoint[0]) {
    return lastPoint
  }

  for (let index = 1; index < points.length; index += 1) {
    const leftPoint = points[index - 1]
    const rightPoint = points[index]
    if (!leftPoint || !rightPoint) {
      continue
    }

    if (rightPoint[0] < safeTargetX) {
      continue
    }

    const deltaX = rightPoint[0] - leftPoint[0]
    if (!Number.isFinite(deltaX) || deltaX === 0) {
      return rightPoint
    }

    const ratio = (safeTargetX - leftPoint[0]) / deltaX
    return [
      leftPoint[0] + (rightPoint[0] - leftPoint[0]) * ratio,
      leftPoint[1] + (rightPoint[1] - leftPoint[1]) * ratio,
    ]
  }

  return lastPoint
}

export function areaToSvg(points, width, height, padding = 18) {
  if (!points.length) return ''
  return [
    `${padding},${height - padding}`,
    ...points.map(([x, y]) => `${x},${y}`),
    `${width - padding},${height - padding}`,
  ].join(' ')
}

export function getCompletedIndex(totalPoints, sampleIndex, progress01) {
  if (totalPoints <= 1) return 0

  if (Number.isFinite(progress01)) {
    return clamp(Math.floor(progress01 * (totalPoints - 1)), 0, totalPoints - 1)
  }

  return clamp(sampleIndex, 0, totalPoints - 1)
}
