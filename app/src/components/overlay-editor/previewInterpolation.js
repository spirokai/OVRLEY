/**
 * Provides overlay editor helpers for preview interpolation.
 */

const NUMERIC_PREVIEW_KEYS = [
  'air_pressure',
  'altitude',
  'cadence',
  'distance',
  'elevation',
  'g_force',
  'gradient',
  'ground_contact_time',
  'heading',
  'heartrate',
  'left_right_balance',
  'pace',
  'power',
  'speed',
  'stroke_rate',
  'stride_length',
  'temperature',
  'torque',
  'vertical_oscillation',
  'vertical_speed',
]

const WIDGET_PREVIEW_DEPENDENCIES = {
  cadence: ['cadence'],
  course: ['course', 'frame_distance_progress'],
  elevation: ['elevation', 'frame_distance_progress'],
  gradient: ['gradient'],
  heartrate: ['heartrate'],
  power: ['power'],
  speed: ['speed'],
  temperature: ['temperature'],
  time: ['time'],
}

const previewActivityCache = new WeakMap()

/**
 * Constrains a value to the provided minimum and maximum bounds.
 *
 * @param {*} value - Input value processed by the helper.
 * @param {*} min - Lower bound used by the calculation.
 * @param {*} max - Upper bound used by the calculation.
 * @returns {number} Result produced by the helper.
 */
function clamp(value, min, max) {
  return Math.min(max, Math.max(min, value))
}

/**
 * Builds target times.
 *
 * @param {*} startSecond - Value for start second.
 * @param {*} endSecond - Value for end second.
 * @param {*} fps - Numeric fps value.
 * @returns {*} Derived data structure for downstream use.
 */
function buildTargetTimes(startSecond, endSecond, fps) {
  const safeFps = Math.max(Number(fps) || 1, 1)
  const duration = Math.max(endSecond - startSecond, 0)
  const frameCount = Math.max(1, Math.ceil(duration * safeFps))

  return Array.from({ length: frameCount + 1 }, (_, index) => {
    return Math.min(startSecond + index / safeFps, endSecond)
  })
}

/**
 * Builds valid numeric samples.
 *
 * @param {*} xValues - Series of x-axis values used for interpolation.
 * @param {*} yValues - Series of y-axis values used for interpolation.
 * @returns {object} Derived data structure for downstream use.
 */
function buildValidNumericSamples(xValues, yValues) {
  const validX = []
  const validY = []

  xValues.forEach((xValue, index) => {
    const yValue = yValues[index]
    if (!Number.isFinite(xValue) || !Number.isFinite(yValue)) {
      return
    }

    validX.push(Number(xValue))
    validY.push(Number(yValue))
  })

  return { validX, validY }
}

/**
 * Handles interpolate sorted numeric series.
 *
 * @param {*} xValues - Series of x-axis values used for interpolation.
 * @param {*} yValues - Series of y-axis values used for interpolation.
 * @param {*} targetXValues - Value for target xvalues.
 * @returns {*} Result produced by the helper.
 */
function interpolateSortedNumericSeries(xValues, yValues, targetXValues) {
  const { validX, validY } = buildValidNumericSamples(xValues, yValues)

  if (!validX.length || !targetXValues.length) {
    return []
  }

  if (validX.length === 1) {
    return targetXValues.map(() => validY[0])
  }

  const interpolated = []
  let sourceIndex = 0

  targetXValues.forEach((targetX) => {
    if (targetX <= validX[0]) {
      interpolated.push(validY[0])
      return
    }

    const lastIndex = validX.length - 1
    if (targetX >= validX[lastIndex]) {
      interpolated.push(validY[lastIndex])
      return
    }

    while (sourceIndex < lastIndex - 1 && validX[sourceIndex + 1] < targetX) {
      sourceIndex += 1
    }

    const leftX = validX[sourceIndex]
    const rightX = validX[sourceIndex + 1]
    const leftY = validY[sourceIndex]
    const rightY = validY[sourceIndex + 1]

    if (!Number.isFinite(rightX - leftX) || rightX === leftX) {
      interpolated.push(rightY)
      return
    }

    const ratio = (targetX - leftX) / (rightX - leftX)
    interpolated.push(leftY + (rightY - leftY) * ratio)
  })

  return interpolated
}

/**
 * Handles interpolate course series.
 *
 * @param {*} xValues - Series of x-axis values used for interpolation.
 * @param {*} courseSeries - Value for course series.
 * @param {*} targetXValues - Value for target xvalues.
 * @returns {*} Result produced by the helper.
 */
function interpolateCourseSeries(xValues, courseSeries, targetXValues) {
  const validX = []
  const latitudes = []
  const longitudes = []

  xValues.forEach((xValue, index) => {
    const point = courseSeries[index]
    if (
      !Number.isFinite(xValue) ||
      !Array.isArray(point) ||
      !Number.isFinite(point[0]) ||
      !Number.isFinite(point[1])
    ) {
      return
    }

    validX.push(Number(xValue))
    latitudes.push(Number(point[0]))
    longitudes.push(Number(point[1]))
  })

  if (!validX.length) {
    return []
  }

  const nextLatitudes = interpolateSortedNumericSeries(
    validX,
    latitudes,
    targetXValues,
  )
  const nextLongitudes = interpolateSortedNumericSeries(
    validX,
    longitudes,
    targetXValues,
  )

  return nextLatitudes.map((latitude, index) => [
    latitude,
    nextLongitudes[index] ?? longitudes[longitudes.length - 1],
  ])
}

/**
 * Handles interpolate time series.
 *
 * @param {*} activity - Parsed activity data for previews or rendering.
 * @param {*} targetXValues - Value for target xvalues.
 * @returns {*} Result produced by the helper.
 */
function interpolateTimeSeries(activity, targetXValues) {
  if (!targetXValues.length) {
    return []
  }

  const sourceStartTimeMs = Date.parse(activity?.source_start_time || '')
  if (Number.isFinite(sourceStartTimeMs)) {
    return targetXValues.map((targetX) =>
      new Date(sourceStartTimeMs + targetX * 1000).toISOString(),
    )
  }

  const timeSeries = Array.isArray(activity?.time) ? activity.time : []
  const elapsedSeries = Array.isArray(activity?.sample_elapsed_seconds)
    ? activity.sample_elapsed_seconds
    : []
  const numericTimeSeries = timeSeries.map((value) => {
    const parsedValue = Date.parse(value || '')
    return Number.isFinite(parsedValue) ? parsedValue : null
  })
  const interpolatedTimeMs = interpolateSortedNumericSeries(
    elapsedSeries,
    numericTimeSeries,
    targetXValues,
  )

  return interpolatedTimeMs.map((value) => {
    if (!Number.isFinite(value)) {
      return null
    }

    return new Date(value).toISOString()
  })
}

/**
 * Returns effective preview fps.
 *
 * @param {*} fps - Numeric fps value.
 * @param {*} updateRate - Metric sampling rate used during export.
 * @returns {*} Requested value or structure.
 */
export function getEffectivePreviewFps(fps, updateRate) {
  const safeSceneFps = Math.max(Number(fps) || 30, 1)
  const safeUpdateRate = Math.max(Number(updateRate) || 1, 1)

  return Math.max(safeSceneFps / safeUpdateRate, 1)
}

/**
 * Returns required preview keys.
 *
 * @param {*} widgets - Widget collection in the current template.
 * @returns {*} Requested value or structure.
 */
export function getRequiredPreviewKeys(widgets) {
  const requiredKeys = new Set()

  widgets.forEach((widget) => {
    const dependencies = WIDGET_PREVIEW_DEPENDENCIES[widget.type] || []
    dependencies.forEach((key) => requiredKeys.add(key))
  })

  return [...requiredKeys].sort()
}

/**
 * Builds preview activity.
 *
 * @param {object} options - Structured options for the helper.
 * @param {*} options.activity - Parsed activity data for previews or rendering.
 * @param {*} options.startSecond - Value for start second.
 * @param {*} options.endSecond - Value for end second.
 * @param {*} options.fps - Numeric fps value.
 * @param {*} options.updateRate - Metric sampling rate used during export.
 * @param {*} options.enabled - Value for enabled.
 * @param {*} options.requiredKeys - Value for required keys.
 * @returns {*} Derived data structure for downstream use.
 */
export function buildPreviewActivity({
  activity,
  startSecond,
  endSecond,
  fps,
  updateRate,
  enabled,
  requiredKeys = [],
}) {
  if (!enabled || !activity) {
    return activity
  }

  const elapsedSeries = Array.isArray(activity.sample_elapsed_seconds)
    ? activity.sample_elapsed_seconds
    : []
  if (elapsedSeries.length < 2) {
    return activity
  }

  const maxAvailableSecond = Number(
    activity.trim_end_seconds ??
      activity.metadata?.duration_seconds ??
      elapsedSeries[elapsedSeries.length - 1] ??
      0,
  )
  const safeStart = clamp(Number(startSecond) || 0, 0, maxAvailableSecond)
  const rawEnd = Number.isFinite(Number(endSecond))
    ? Number(endSecond)
    : maxAvailableSecond
  const safeEnd = clamp(rawEnd, safeStart, maxAvailableSecond)

  if (safeEnd <= safeStart) {
    return activity
  }

  const effectivePreviewFps = getEffectivePreviewFps(fps, updateRate)
  const requiredKeySignature = [...requiredKeys].sort().join('|')
  const cacheKey = [
    safeStart,
    safeEnd,
    effectivePreviewFps,
    requiredKeySignature,
  ].join(':')
  const cachedByActivity = previewActivityCache.get(activity)

  if (cachedByActivity?.has(cacheKey)) {
    return cachedByActivity.get(cacheKey)
  }

  const frameElapsedSeconds = buildTargetTimes(
    safeStart,
    safeEnd,
    effectivePreviewFps,
  )

  if (frameElapsedSeconds.length < 2) {
    return activity
  }

  const previewActivity = {
    ...activity,
    frame_elapsed_seconds: frameElapsedSeconds,
  }

  const requestedKeys = requiredKeys.length
    ? new Set(requiredKeys)
    : new Set([
        ...NUMERIC_PREVIEW_KEYS,
        'course',
        'frame_distance_progress',
        'time',
      ])

  if (requestedKeys.has('time')) {
    const interpolatedTimeSeries = interpolateTimeSeries(
      activity,
      frameElapsedSeconds,
    )
    previewActivity.frame_timestamps = interpolatedTimeSeries
    previewActivity.time = interpolatedTimeSeries
  }

  if (requestedKeys.has('frame_distance_progress')) {
    previewActivity.frame_distance_progress = interpolateSortedNumericSeries(
      elapsedSeries,
      Array.isArray(activity.sample_distance_progress)
        ? activity.sample_distance_progress
        : [],
      frameElapsedSeconds,
    )
  }

  NUMERIC_PREVIEW_KEYS.forEach((key) => {
    if (!requestedKeys.has(key)) {
      return
    }

    if (!Array.isArray(activity[key])) {
      return
    }

    previewActivity[key] = interpolateSortedNumericSeries(
      elapsedSeries,
      activity[key],
      frameElapsedSeconds,
    )
  })

  if (requestedKeys.has('course') && Array.isArray(activity.course)) {
    previewActivity.course = interpolateCourseSeries(
      elapsedSeries,
      activity.course,
      frameElapsedSeconds,
    )
  }

  const nextCachedByActivity = cachedByActivity || new Map()
  nextCachedByActivity.set(cacheKey, previewActivity)
  if (!cachedByActivity) {
    previewActivityCache.set(activity, nextCachedByActivity)
  }

  return previewActivity
}
