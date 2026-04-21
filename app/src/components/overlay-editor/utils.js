import { DEFAULT_ACTIVITY_PREVIEW, FONT_FAMILY_MAP } from './constants'

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

export function normalizeRoutePoints(points, width, height, padding = 18) {
  const validPoints = points.filter(
    ([latitude, longitude]) =>
      Number.isFinite(latitude) && Number.isFinite(longitude),
  )

  if (validPoints.length < 2) {
    return buildFallbackRoute(width, height)
  }

  const latitudes = validPoints.map(([latitude]) => latitude)
  const meanLatitude =
    latitudes.reduce((sum, latitude) => sum + latitude, 0) / latitudes.length
  const meanLatitudeRadians = meanLatitude * (Math.PI / 180)
  const projectedPoints = validPoints.map(([latitude, longitude]) => [
    latitude,
    longitude * Math.cos(meanLatitudeRadians),
  ])
  const projectedLongitudes = projectedPoints.map(([, longitude]) => longitude)
  const minLatitude = Math.min(...latitudes)
  const maxLatitude = Math.max(...latitudes)
  const minLongitude = Math.min(...projectedLongitudes)
  const maxLongitude = Math.max(...projectedLongitudes)
  const usableWidth = Math.max(width - padding * 2, 1)
  const usableHeight = Math.max(height - padding * 2, 1)
  const longitudeRange = Math.max(maxLongitude - minLongitude, 0.000001)
  const latitudeRange = Math.max(maxLatitude - minLatitude, 0.000001)
  const scale = Math.min(
    usableWidth / longitudeRange,
    usableHeight / latitudeRange,
  )
  const contentWidth = longitudeRange * scale
  const contentHeight = latitudeRange * scale
  const offsetX = (width - contentWidth) / 2
  const offsetY = (height - contentHeight) / 2

  return projectedPoints.map(([latitude, longitude]) => {
    const x = offsetX + (longitude - minLongitude) * scale
    const y = height - (offsetY + (latitude - minLatitude) * scale)
    return [x, y]
  })
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
) {
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
  const safeVerticalScale = clamp(Number(verticalScale) || 1, 0.2, 4)

  return usableValues.map((value, index) => {
    const x = padding + index * step
    const normalized = amplitude <= 0 ? 0.5 : (value - minimum) / amplitude
    const centered = clamp((normalized - 0.5) * safeVerticalScale + 0.5, 0, 1)
    const y = height - padding - centered * (height - padding * 2)
    return [x, y]
  })
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
