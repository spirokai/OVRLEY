import { DEFAULT_ACTIVITY_PREVIEW, FONT_FAMILY_MAP } from './constants'

export function clamp(value, min, max) {
  return Math.min(max, Math.max(min, value))
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

export function getSampleValue(activity, key, sampleIndex) {
  const series = activity?.[key]
  if (!Array.isArray(series)) {
    return DEFAULT_ACTIVITY_PREVIEW[key] ?? null
  }

  return series[sampleIndex] ?? DEFAULT_ACTIVITY_PREVIEW[key] ?? null
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

export function normalizeElevationPoints(values, width, height, padding = 18) {
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

export function pointsToSvg(points) {
  return points.map(([x, y]) => `${x},${y}`).join(' ')
}

export function areaToSvg(points, width, height, padding = 18) {
  if (!points.length) return ''
  return [
    `${padding},${height - padding}`,
    ...points.map(([x, y]) => `${x},${y}`),
    `${width - padding},${height - padding}`,
  ].join(' ')
}

export function getCompletedIndex(totalPoints, sampleIndex) {
  if (totalPoints <= 1) return 0
  return clamp(sampleIndex, 0, totalPoints - 1)
}
