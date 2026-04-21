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

function clamp(value, min, max) {
  return Math.min(max, Math.max(min, value))
}

function buildTargetTimes(startSecond, endSecond, fps) {
  const safeFps = Math.max(Number(fps) || 1, 1)
  const duration = Math.max(endSecond - startSecond, 0)
  const frameCount = Math.max(1, Math.ceil(duration * safeFps))

  return Array.from({ length: frameCount + 1 }, (_, index) => {
    return Math.min(startSecond + index / safeFps, endSecond)
  })
}

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

export function getEffectivePreviewFps(fps, updateRate) {
  const safeSceneFps = Math.max(Number(fps) || 30, 1)
  const safeUpdateRate = Math.max(Number(updateRate) || 1, 1)

  return Math.max(safeSceneFps / safeUpdateRate, 1)
}

export function buildPreviewActivity({
  activity,
  startSecond,
  endSecond,
  fps,
  updateRate,
  enabled,
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
    frame_timestamps: interpolateTimeSeries(activity, frameElapsedSeconds),
    time: interpolateTimeSeries(activity, frameElapsedSeconds),
    frame_distance_progress: interpolateSortedNumericSeries(
      elapsedSeries,
      Array.isArray(activity.sample_distance_progress)
        ? activity.sample_distance_progress
        : [],
      frameElapsedSeconds,
    ),
  }

  NUMERIC_PREVIEW_KEYS.forEach((key) => {
    if (!Array.isArray(activity[key])) {
      return
    }

    previewActivity[key] = interpolateSortedNumericSeries(
      elapsedSeries,
      activity[key],
      frameElapsedSeconds,
    )
  })

  if (Array.isArray(activity.course)) {
    previewActivity.course = interpolateCourseSeries(
      elapsedSeries,
      activity.course,
      frameElapsedSeconds,
    )
  }

  return previewActivity
}
