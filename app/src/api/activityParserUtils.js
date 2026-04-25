export const CORE_ACTIVITY_ATTRIBUTES = [
  'cadence',
  'course',
  'elevation',
  'gradient',
  'heartrate',
  'power',
  'speed',
  'time',
  'temperature',
]

const EXTENDED_ACTIVITY_ATTRIBUTES = [
  'air_pressure',
  'altitude',
  'distance',
  'g_force',
  'ground_contact_time',
  'heading',
  'left_right_balance',
  'pace',
  'stroke_rate',
  'stride_length',
  'torque',
  'vertical_oscillation',
  'vertical_speed',
]

const METRIC_UNITS = {
  air_pressure: 'bar',
  altitude: 'm',
  cadence: 'rpm',
  distance: 'm',
  elevation: 'm',
  g_force: 'g',
  gradient: 'percent',
  ground_contact_time: 'ms',
  heading: 'degrees',
  heartrate: 'bpm',
  left_right_balance: 'raw',
  pace: 'seconds_per_km',
  power: 'watts',
  speed: 'mps',
  stride_length: 'raw',
  stroke_rate: 'strokes_per_minute',
  temperature: 'celsius',
  torque: 'nm',
  vertical_oscillation: 'raw',
  vertical_speed: 'mps',
}

function isFiniteNumber(value) {
  return typeof value === 'number' && Number.isFinite(value)
}

function roundValue(value, digits = 6) {
  if (!isFiniteNumber(value)) return null
  const scale = 10 ** digits
  return Math.round(value * scale) / scale
}

export function safeNumber(value) {
  if (value === null || value === undefined || value === '') {
    return null
  }

  if (typeof value === 'number') {
    return Number.isFinite(value) ? value : null
  }

  if (typeof value === 'string') {
    const normalized = value.trim()
    if (!normalized) return null
    const numeric = Number(normalized)
    return Number.isFinite(numeric) ? numeric : null
  }

  return null
}

export function safeTimestamp(value) {
  if (!value) return null
  const date = value instanceof Date ? value : new Date(value)
  if (!Number.isFinite(date.getTime())) return null
  return date.toISOString()
}

export function haversineDistanceMeters(lat1, lon1, lat2, lon2) {
  if (
    !isFiniteNumber(lat1) ||
    !isFiniteNumber(lon1) ||
    !isFiniteNumber(lat2) ||
    !isFiniteNumber(lon2)
  ) {
    return 0
  }

  const earthRadiusMeters = 6371000
  const lat1Rad = (lat1 * Math.PI) / 180
  const lat2Rad = (lat2 * Math.PI) / 180
  const latDeltaRad = ((lat2 - lat1) * Math.PI) / 180
  const lonDeltaRad = ((lon2 - lon1) * Math.PI) / 180

  const a =
    Math.sin(latDeltaRad / 2) ** 2 +
    Math.cos(lat1Rad) * Math.cos(lat2Rad) * Math.sin(lonDeltaRad / 2) ** 2

  const c = 2 * Math.atan2(Math.sqrt(a), Math.sqrt(1 - a))
  return earthRadiusMeters * c
}

function calculateBearingDegrees(fromPoint, toPoint) {
  if (!fromPoint || !toPoint) return null

  const [fromLat, fromLon] = fromPoint
  const [toLat, toLon] = toPoint
  if (
    !isFiniteNumber(fromLat) ||
    !isFiniteNumber(fromLon) ||
    !isFiniteNumber(toLat) ||
    !isFiniteNumber(toLon)
  ) {
    return null
  }

  const fromLatRad = (fromLat * Math.PI) / 180
  const toLatRad = (toLat * Math.PI) / 180
  const lonDeltaRad = ((toLon - fromLon) * Math.PI) / 180
  const y = Math.sin(lonDeltaRad) * Math.cos(toLatRad)
  const x =
    Math.cos(fromLatRad) * Math.sin(toLatRad) -
    Math.sin(fromLatRad) * Math.cos(toLatRad) * Math.cos(lonDeltaRad)

  const bearing = (Math.atan2(y, x) * 180) / Math.PI
  return (bearing + 360) % 360
}

function buildDistanceSeries(coursePoints, directDistanceSeries) {
  const distanceSeries = []
  let totalDistanceMeters = 0

  for (let index = 0; index < coursePoints.length; index += 1) {
    const directDistance = directDistanceSeries[index]
    if (isFiniteNumber(directDistance)) {
      totalDistanceMeters = Math.max(totalDistanceMeters, directDistance)
      distanceSeries.push(roundValue(totalDistanceMeters, 3))
      continue
    }

    const previousPoint = index > 0 ? coursePoints[index - 1] : null
    const currentPoint = coursePoints[index]
    if (index > 0) {
      totalDistanceMeters += haversineDistanceMeters(
        previousPoint?.[0],
        previousPoint?.[1],
        currentPoint?.[0],
        currentPoint?.[1],
      )
    }

    distanceSeries.push(roundValue(totalDistanceMeters, 3))
  }

  return distanceSeries
}

function buildElapsedSeries(rawSamples, timeSeries) {
  const explicitElapsed = rawSamples.map((sample) =>
    safeNumber(sample.elapsedSeconds),
  )
  const hasExplicitElapsed = explicitElapsed.some(isFiniteNumber)

  if (hasExplicitElapsed) {
    const elapsedSeries = []
    let lastValue = 0

    for (let index = 0; index < explicitElapsed.length; index += 1) {
      const currentExplicit = explicitElapsed[index]
      if (isFiniteNumber(currentExplicit)) {
        lastValue = Math.max(lastValue, currentExplicit)
        elapsedSeries.push(roundValue(lastValue, 3))
        continue
      }

      if (index === 0) {
        elapsedSeries.push(0)
        continue
      }

      lastValue = elapsedSeries[index - 1]
      elapsedSeries.push(roundValue(lastValue, 3))
    }

    return elapsedSeries
  }

  const validTimestamps = timeSeries.map((value) =>
    value ? new Date(value) : null,
  )
  const origin = validTimestamps.find(
    (value) => value && Number.isFinite(value.getTime()),
  )

  if (!origin) {
    return rawSamples.map((_, index) => roundValue(index, 3))
  }

  let lastValue = 0
  return validTimestamps.map((timestamp, index) => {
    if (!timestamp || !Number.isFinite(timestamp.getTime())) {
      lastValue = index === 0 ? 0 : lastValue
      return roundValue(lastValue, 3)
    }

    const nextValue = Math.max(
      0,
      (timestamp.getTime() - origin.getTime()) / 1000,
    )

    if (nextValue <= lastValue && index > 0) {
      lastValue += 0.001
      return roundValue(lastValue, 3)
    }

    lastValue = nextValue
    return roundValue(nextValue, 3)
  })
}

function buildProgressSeries(distanceSeries) {
  const totalDistanceMeters = distanceSeries[distanceSeries.length - 1] ?? 0
  if (!isFiniteNumber(totalDistanceMeters) || totalDistanceMeters <= 0) {
    return distanceSeries.map(() => 0)
  }

  return distanceSeries.map((value) =>
    roundValue(value / totalDistanceMeters, 6),
  )
}

function smoothElevationSeries(elevationSeries, radius = 2) {
  return elevationSeries.map((value, index) => {
    if (!isFiniteNumber(value)) return null

    let total = 0
    let count = 0
    for (
      let neighborIndex = Math.max(0, index - radius);
      neighborIndex <= Math.min(elevationSeries.length - 1, index + radius);
      neighborIndex += 1
    ) {
      const neighborValue = elevationSeries[neighborIndex]
      if (!isFiniteNumber(neighborValue)) continue
      total += neighborValue
      count += 1
    }

    return count > 0 ? roundValue(total / count, 3) : null
  })
}

function applyFixedSavitzkyGolay(values, coefficients) {
  const radius = Math.floor(coefficients.length / 2)
  return values.map((value, index) => {
    if (!isFiniteNumber(value)) return null

    let total = 0
    let coefficientTotal = 0
    for (let offset = -radius; offset <= radius; offset += 1) {
      const neighborIndex = index + offset
      if (neighborIndex < 0 || neighborIndex >= values.length) continue

      const neighborValue = values[neighborIndex]
      if (!isFiniteNumber(neighborValue)) continue

      const coefficient = coefficients[offset + radius]
      total += neighborValue * coefficient
      coefficientTotal += coefficient
    }

    if (!coefficientTotal) {
      return roundValue(value, 3)
    }

    return roundValue(total / coefficientTotal, 3)
  })
}

function smoothElevationSeriesLegacy(elevationSeries) {
  const populatedSamples = elevationSeries.filter(isFiniteNumber).length
  if (populatedSamples < 3) {
    return elevationSeries.map((value) => roundValue(value, 3))
  }

  return applyFixedSavitzkyGolay(
    elevationSeries,
    [-36, 9, 44, 69, 84, 89, 84, 69, 44, 9, -36],
  )
}

function smoothGradientInputSeriesLegacy(elevationSeries) {
  const populatedSamples = elevationSeries.filter(isFiniteNumber).length
  if (populatedSamples < 3) {
    return elevationSeries.map((value) => roundValue(value, 3))
  }

  return applyFixedSavitzkyGolay(elevationSeries, [-2, 3, 6, 7, 6, 3, -2])
}

function deriveGradientSeries(elevationSeries, distanceSeries) {
  const smoothedElevation = smoothElevationSeries(elevationSeries)
  const gradientSeries = []
  let lastGradient = 0

  for (let index = 0; index < distanceSeries.length; index += 1) {
    const currentDistance = distanceSeries[index]
    if (!isFiniteNumber(currentDistance)) {
      gradientSeries.push(null)
      continue
    }

    let leftIndex = index
    while (leftIndex > 0 && currentDistance - distanceSeries[leftIndex] < 5) {
      leftIndex -= 1
    }

    let rightIndex = index
    while (
      rightIndex < distanceSeries.length - 1 &&
      distanceSeries[rightIndex] - currentDistance < 5
    ) {
      rightIndex += 1
    }

    const leftDistance = distanceSeries[leftIndex]
    const rightDistance = distanceSeries[rightIndex]
    const leftElevation = smoothedElevation[leftIndex]
    const rightElevation = smoothedElevation[rightIndex]

    if (
      !isFiniteNumber(leftDistance) ||
      !isFiniteNumber(rightDistance) ||
      !isFiniteNumber(leftElevation) ||
      !isFiniteNumber(rightElevation)
    ) {
      gradientSeries.push(roundValue(lastGradient, 3))
      continue
    }

    const horizontalDistance = rightDistance - leftDistance
    if (horizontalDistance < 1) {
      gradientSeries.push(roundValue(lastGradient, 3))
      continue
    }

    const nextGradient = Math.max(
      -30,
      Math.min(
        30,
        ((rightElevation - leftElevation) / horizontalDistance) * 100,
      ),
    )
    lastGradient = nextGradient
    gradientSeries.push(roundValue(nextGradient, 3))
  }

  return gradientSeries
}

function deriveLegacyGradientSeries(elevationSeries, distanceSeries) {
  if (!elevationSeries.length) {
    return []
  }

  if (
    elevationSeries.length === 1 ||
    distanceSeries.length !== elevationSeries.length
  ) {
    return elevationSeries.map(() => 0)
  }

  const gradientInputElevations =
    smoothGradientInputSeriesLegacy(elevationSeries)
  const gradients = []
  const lastIndex = elevationSeries.length - 1

  for (let index = 0; index < elevationSeries.length; index += 1) {
    const leftIndex = index === 0 ? 0 : index - 1
    const rightIndex = index === lastIndex ? lastIndex : index + 1

    const leftDistance = distanceSeries[leftIndex]
    const rightDistance = distanceSeries[rightIndex]
    const leftElevation = gradientInputElevations[leftIndex]
    const rightElevation = gradientInputElevations[rightIndex]

    if (
      !isFiniteNumber(leftDistance) ||
      !isFiniteNumber(rightDistance) ||
      !isFiniteNumber(leftElevation) ||
      !isFiniteNumber(rightElevation)
    ) {
      gradients.push(0)
      continue
    }

    const horizontalDistance = rightDistance - leftDistance
    if (horizontalDistance <= 0) {
      gradients.push(0)
      continue
    }

    gradients.push(
      roundValue(
        ((rightElevation - leftElevation) / horizontalDistance) * 100,
        3,
      ) ?? 0,
    )
  }

  return gradients
}

function deriveHeadingSeries(courseSeries) {
  let lastHeading = null

  return courseSeries.map((point, index) => {
    const previousPoint = index > 0 ? courseSeries[index - 1] : null
    const nextPoint =
      index < courseSeries.length - 1 ? courseSeries[index + 1] : null
    const heading =
      calculateBearingDegrees(previousPoint, point) ??
      calculateBearingDegrees(point, nextPoint) ??
      lastHeading

    if (isFiniteNumber(heading)) {
      lastHeading = heading
      return roundValue(heading, 3)
    }

    return null
  })
}

function deriveNumericRateSeries(numeratorSeries, elapsedSeries) {
  const derivedSeries = []
  let lastValue = null

  for (let index = 0; index < numeratorSeries.length; index += 1) {
    if (index === 0) {
      derivedSeries.push(null)
      continue
    }

    const previousValue = numeratorSeries[index - 1]
    const currentValue = numeratorSeries[index]
    const previousElapsed = elapsedSeries[index - 1]
    const currentElapsed = elapsedSeries[index]

    if (
      !isFiniteNumber(previousValue) ||
      !isFiniteNumber(currentValue) ||
      !isFiniteNumber(previousElapsed) ||
      !isFiniteNumber(currentElapsed)
    ) {
      derivedSeries.push(lastValue)
      continue
    }

    const elapsedDelta = currentElapsed - previousElapsed
    if (elapsedDelta <= 0) {
      derivedSeries.push(lastValue)
      continue
    }

    lastValue = (currentValue - previousValue) / elapsedDelta
    derivedSeries.push(roundValue(lastValue, 6))
  }

  return derivedSeries
}

function derivePaceSeries(speedSeries) {
  return speedSeries.map((speed) => {
    if (!isFiniteNumber(speed) || speed <= 0) return null
    return roundValue(1000 / speed, 3)
  })
}

function deriveTorqueSeries(powerSeries, cadenceSeries) {
  return powerSeries.map((power, index) => {
    const cadence = cadenceSeries[index]
    if (!isFiniteNumber(power) || !isFiniteNumber(cadence) || cadence <= 0) {
      return null
    }

    const angularVelocity = (2 * Math.PI * cadence) / 60
    if (!isFiniteNumber(angularVelocity) || angularVelocity <= 0) return null
    return roundValue(power / angularVelocity, 6)
  })
}

function combineSeries(directSeries, derivedSeries) {
  const combinedSeries = directSeries.map(
    (value, index) => value ?? derivedSeries[index] ?? null,
  )

  const directCount = directSeries.filter((value) => value !== null).length
  const derivedOnlyCount = combinedSeries.filter(
    (value, index) => value !== null && directSeries[index] === null,
  ).length

  let source = 'missing'
  if (directCount > 0 && derivedOnlyCount > 0) {
    source = 'mixed'
  } else if (directCount > 0) {
    source = 'direct'
  } else if (derivedOnlyCount > 0) {
    source = 'derived'
  }

  return {
    series: combinedSeries,
    source,
  }
}

function combineSeriesPreferDerived(derivedSeries, directSeries) {
  const combinedSeries = derivedSeries.map(
    (value, index) => value ?? directSeries[index] ?? null,
  )

  const derivedCount = derivedSeries.filter((value) => value !== null).length
  const directFallbackCount = combinedSeries.filter(
    (value, index) => value !== null && derivedSeries[index] === null,
  ).length

  let source = 'missing'
  if (derivedCount > 0 && directFallbackCount > 0) {
    source = 'mixed'
  } else if (derivedCount > 0) {
    source = 'derived'
  } else if (directFallbackCount > 0) {
    source = 'direct'
  }

  return {
    series: combinedSeries,
    source,
  }
}

function buildMetricCoverage(metricSeriesMap) {
  return Object.fromEntries(
    Object.entries(metricSeriesMap).map(([metric, descriptor]) => {
      const values = descriptor.series
      const availableCount = values.filter((value) => value !== null).length
      return [
        metric,
        {
          source: descriptor.source,
          availableCount,
          totalSamples: values.length,
        },
      ]
    }),
  )
}

function median(values) {
  if (!values.length) return null
  const sorted = [...values].sort((left, right) => left - right)
  const middle = Math.floor(sorted.length / 2)
  if (sorted.length % 2 === 0) {
    return (sorted[middle - 1] + sorted[middle]) / 2
  }
  return sorted[middle]
}

function lowerHalfMedian(values) {
  if (!values.length) return null
  const sorted = [...values].sort((left, right) => left - right)
  const cutoff = Math.max(1, Math.ceil(sorted.length / 2))
  return median(sorted.slice(0, cutoff))
}

function timestampMs(value) {
  if (!value) return null
  const date = value instanceof Date ? value : new Date(value)
  const time = date.getTime()
  return Number.isFinite(time) ? time : null
}

function cloneRawSample(sample) {
  return { ...sample }
}

function zeroFilledIdleSample(sample, elapsedSeconds, timestampMsValue) {
  const synthetic = cloneRawSample(sample)
  synthetic.elapsedSeconds = roundValue(elapsedSeconds, 3)
  synthetic.timestamp =
    timestampMsValue === null ? null : new Date(timestampMsValue).toISOString()
  synthetic.speed = 0
  synthetic.cadence = 0
  synthetic.power = 0
  synthetic.strokeRate = 0
  synthetic.verticalSpeed = 0
  synthetic.gForce = 0
  synthetic.gradient = 0
  synthetic.pace = null
  synthetic.torque = null
  synthetic.strideLength = null
  synthetic.groundContactTime = null
  synthetic.verticalOscillation = null
  synthetic.syntheticIdle = true
  return synthetic
}

function estimateRecordingIntervalSeconds(rawSamples) {
  const deltas = []
  let previousElapsed = null
  let previousTimestampMs = null

  rawSamples.forEach((sample) => {
    const elapsed = safeNumber(sample.elapsedSeconds)
    if (isFiniteNumber(elapsed) && isFiniteNumber(previousElapsed)) {
      const delta = elapsed - previousElapsed
      if (delta > 0 && delta <= 10) deltas.push(delta)
      previousElapsed = elapsed
    } else if (isFiniteNumber(elapsed)) {
      previousElapsed = elapsed
    }

    const currentTimestampMs = timestampMs(sample.timestamp)
    if (currentTimestampMs !== null && previousTimestampMs !== null) {
      const delta = (currentTimestampMs - previousTimestampMs) / 1000
      if (delta > 0 && delta <= 10) deltas.push(delta)
      previousTimestampMs = currentTimestampMs
    } else if (currentTimestampMs !== null) {
      previousTimestampMs = currentTimestampMs
    }
  })

  return lowerHalfMedian(deltas) ?? 1
}

function elapsedSecondsForSample(sample, fallbackOriginTimestampMs) {
  const explicit = safeNumber(sample.elapsedSeconds)
  if (isFiniteNumber(explicit)) return explicit

  const timeMs = timestampMs(sample.timestamp)
  if (timeMs !== null && fallbackOriginTimestampMs !== null) {
    return (timeMs - fallbackOriginTimestampMs) / 1000
  }

  return null
}

function distanceMetersForPair(previousSample, currentSample) {
  const previousDistance = safeNumber(previousSample.distance)
  const currentDistance = safeNumber(currentSample.distance)
  if (isFiniteNumber(previousDistance) && isFiniteNumber(currentDistance)) {
    return Math.max(0, currentDistance - previousDistance)
  }

  return haversineDistanceMeters(
    safeNumber(previousSample.latitude),
    safeNumber(previousSample.longitude),
    safeNumber(currentSample.latitude),
    safeNumber(currentSample.longitude),
  )
}

function insertIdleGapSamples(rawSamples) {
  if (rawSamples.length < 2) {
    return {
      rawSamples,
      gapDebug: {
        detected_gaps: [],
        inserted_sample_count: 0,
        recording_interval_seconds: 1,
      },
    }
  }

  const originTimestampMs = rawSamples
    .map((sample) => timestampMs(sample.timestamp))
    .find((value) => value !== null)
  const recordingIntervalSeconds = Math.max(
    0.2,
    estimateRecordingIntervalSeconds(rawSamples),
  )
  const gapThresholdSeconds = Math.max(3, recordingIntervalSeconds * 3)
  const stationaryDistanceThresholdMeters = Math.max(
    5,
    recordingIntervalSeconds * 2.5,
  )
  const detectedGaps = []
  const filledSamples = [cloneRawSample(rawSamples[0])]
  let insertedSampleCount = 0

  for (let index = 1; index < rawSamples.length; index += 1) {
    const previousSample = rawSamples[index - 1]
    const currentSample = rawSamples[index]
    const previousElapsed = elapsedSecondsForSample(
      previousSample,
      originTimestampMs,
    )
    const currentElapsed = elapsedSecondsForSample(
      currentSample,
      originTimestampMs,
    )
    const elapsedDelta =
      isFiniteNumber(previousElapsed) && isFiniteNumber(currentElapsed)
        ? currentElapsed - previousElapsed
        : null

    let insertedForGap = 0
    if (isFiniteNumber(elapsedDelta) && elapsedDelta > gapThresholdSeconds) {
      const distanceDelta = distanceMetersForPair(previousSample, currentSample)
      if (distanceDelta <= stationaryDistanceThresholdMeters) {
        const previousTimestampMs = timestampMs(previousSample.timestamp)
        const currentTimestampMs = timestampMs(currentSample.timestamp)
        const maxInsertionCount =
          Math.floor(elapsedDelta / recordingIntervalSeconds) - 1

        for (
          let insertIndex = 1;
          insertIndex <= maxInsertionCount;
          insertIndex += 1
        ) {
          const syntheticElapsed =
            previousElapsed + recordingIntervalSeconds * insertIndex
          if (syntheticElapsed >= currentElapsed - 1e-6) break

          let syntheticTimestampMs = null
          if (
            previousTimestampMs !== null &&
            currentTimestampMs !== null &&
            currentTimestampMs > previousTimestampMs
          ) {
            syntheticTimestampMs = Math.min(
              currentTimestampMs,
              previousTimestampMs +
                Math.round(recordingIntervalSeconds * 1000 * insertIndex),
            )
          }

          filledSamples.push(
            zeroFilledIdleSample(
              previousSample,
              syntheticElapsed,
              syntheticTimestampMs,
            ),
          )
          insertedForGap += 1
        }
      }
    }

    if (insertedForGap > 0) {
      detectedGaps.push({
        start_index: index - 1,
        end_index: index,
        start_elapsed_seconds: roundValue(previousElapsed, 3),
        end_elapsed_seconds: roundValue(currentElapsed, 3),
        gap_seconds: roundValue(elapsedDelta, 3),
        inserted_samples: insertedForGap,
        start_timestamp: safeTimestamp(previousSample.timestamp),
        end_timestamp: safeTimestamp(currentSample.timestamp),
      })
      insertedSampleCount += insertedForGap
    }

    filledSamples.push(cloneRawSample(currentSample))
  }

  return {
    rawSamples: filledSamples,
    gapDebug: {
      detected_gaps: detectedGaps,
      inserted_sample_count: insertedSampleCount,
      recording_interval_seconds: roundValue(recordingIntervalSeconds, 3),
      gap_threshold_seconds: roundValue(gapThresholdSeconds, 3),
      stationary_distance_threshold_m: roundValue(
        stationaryDistanceThresholdMeters,
        3,
      ),
    },
  }
}

export function finalizeParsedActivity({
  fileName,
  fileFormat,
  metadata = {},
  rawSamples = [],
  options = {},
}) {
  const useLegacyGpxDerivations = options.useLegacyGpxDerivations === true
  const { rawSamples: normalizedRawSamples, gapDebug } =
    insertIdleGapSamples(rawSamples)
  const timeSeries = normalizedRawSamples.map((sample) =>
    safeTimestamp(sample.timestamp),
  )
  const courseSeries = normalizedRawSamples.map((sample) => {
    const latitude = safeNumber(sample.latitude)
    const longitude = safeNumber(sample.longitude)
    return [latitude, longitude]
  })
  const directDistanceSeries = normalizedRawSamples.map((sample) =>
    safeNumber(sample.distance),
  )
  const distanceSeries = buildDistanceSeries(courseSeries, directDistanceSeries)
  const elapsedSeries = buildElapsedSeries(normalizedRawSamples, timeSeries)
  const elevationBaseSeries = normalizedRawSamples.map((sample) =>
    safeNumber(sample.elevation),
  )
  const normalizedElevationSeries = useLegacyGpxDerivations
    ? smoothElevationSeriesLegacy(elevationBaseSeries)
    : elevationBaseSeries

  const directMetrics = {
    air_pressure: normalizedRawSamples.map((sample) =>
      safeNumber(sample.airPressure),
    ),
    altitude: normalizedRawSamples.map((sample) => safeNumber(sample.altitude)),
    cadence: normalizedRawSamples.map((sample) => safeNumber(sample.cadence)),
    distance: distanceSeries,
    elevation: normalizedElevationSeries,
    g_force: normalizedRawSamples.map((sample) => safeNumber(sample.gForce)),
    gradient: normalizedRawSamples.map((sample) => safeNumber(sample.gradient)),
    ground_contact_time: normalizedRawSamples.map((sample) =>
      safeNumber(sample.groundContactTime),
    ),
    heading: normalizedRawSamples.map((sample) => safeNumber(sample.heading)),
    heartrate: normalizedRawSamples.map((sample) =>
      safeNumber(sample.heartrate),
    ),
    left_right_balance: normalizedRawSamples.map(
      (sample) => sample.leftRightBalance ?? null,
    ),
    pace: normalizedRawSamples.map((sample) => safeNumber(sample.pace)),
    power: normalizedRawSamples.map((sample) => safeNumber(sample.power)),
    speed: normalizedRawSamples.map((sample) => safeNumber(sample.speed)),
    stride_length: normalizedRawSamples.map((sample) =>
      safeNumber(sample.strideLength),
    ),
    stroke_rate: normalizedRawSamples.map((sample) =>
      safeNumber(sample.strokeRate),
    ),
    temperature: normalizedRawSamples.map((sample) =>
      safeNumber(sample.temperature),
    ),
    torque: normalizedRawSamples.map((sample) => safeNumber(sample.torque)),
    vertical_oscillation: normalizedRawSamples.map((sample) =>
      safeNumber(sample.verticalOscillation),
    ),
    vertical_speed: normalizedRawSamples.map((sample) =>
      safeNumber(sample.verticalSpeed),
    ),
  }

  const derivedSpeed = deriveNumericRateSeries(distanceSeries, elapsedSeries)
  const derivedHeading = deriveHeadingSeries(courseSeries)
  const derivedGradient = useLegacyGpxDerivations
    ? deriveLegacyGradientSeries(directMetrics.elevation, distanceSeries)
    : deriveGradientSeries(directMetrics.elevation, distanceSeries)
  const derivedVerticalSpeed = deriveNumericRateSeries(
    directMetrics.elevation,
    elapsedSeries,
  )
  const derivedPace = derivePaceSeries(
    directMetrics.speed.map((value, index) => value ?? derivedSpeed[index]),
  )
  const derivedTorque = deriveTorqueSeries(
    directMetrics.power,
    directMetrics.cadence,
  )

  const metricSeriesMap = {
    air_pressure: combineSeries(
      directMetrics.air_pressure,
      normalizedRawSamples.map(() => null),
    ),
    altitude: combineSeries(directMetrics.altitude, directMetrics.elevation),
    cadence: combineSeries(
      directMetrics.cadence,
      normalizedRawSamples.map(() => null),
    ),
    distance: { series: directMetrics.distance, source: 'direct' },
    elevation: combineSeries(
      directMetrics.elevation,
      normalizedRawSamples.map(() => null),
    ),
    g_force: combineSeries(
      directMetrics.g_force,
      normalizedRawSamples.map(() => null),
    ),
    gradient: combineSeriesPreferDerived(
      derivedGradient,
      directMetrics.gradient,
    ),
    ground_contact_time: combineSeries(
      directMetrics.ground_contact_time,
      normalizedRawSamples.map(() => null),
    ),
    heading: combineSeries(directMetrics.heading, derivedHeading),
    heartrate: combineSeries(
      directMetrics.heartrate,
      normalizedRawSamples.map(() => null),
    ),
    left_right_balance: combineSeries(
      directMetrics.left_right_balance,
      normalizedRawSamples.map(() => null),
    ),
    pace: combineSeries(directMetrics.pace, derivedPace),
    power: combineSeries(
      directMetrics.power,
      normalizedRawSamples.map(() => null),
    ),
    speed: combineSeries(directMetrics.speed, derivedSpeed),
    stride_length: combineSeries(
      directMetrics.stride_length,
      normalizedRawSamples.map(() => null),
    ),
    stroke_rate: combineSeries(
      directMetrics.stroke_rate,
      normalizedRawSamples.map(() => null),
    ),
    temperature: combineSeries(
      directMetrics.temperature,
      normalizedRawSamples.map(() => null),
    ),
    torque: combineSeries(directMetrics.torque, derivedTorque),
    vertical_oscillation: combineSeries(
      directMetrics.vertical_oscillation,
      normalizedRawSamples.map(() => null),
    ),
    vertical_speed: combineSeries(
      directMetrics.vertical_speed,
      derivedVerticalSpeed,
    ),
  }

  const validAttributes = CORE_ACTIVITY_ATTRIBUTES.filter((attribute) => {
    if (attribute === 'course') {
      return courseSeries.some(
        ([latitude, longitude]) =>
          isFiniteNumber(latitude) && isFiniteNumber(longitude),
      )
    }

    if (attribute === 'time') {
      return timeSeries.some(Boolean)
    }

    return metricSeriesMap[attribute].series.some((value) => value !== null)
  })

  const extendedAttributes = EXTENDED_ACTIVITY_ATTRIBUTES.filter((attribute) =>
    metricSeriesMap[attribute].series.some((value) => value !== null),
  )

  const durationSeconds = elapsedSeries[elapsedSeries.length - 1] ?? 0
  const totalDistanceMeters = distanceSeries[distanceSeries.length - 1] ?? 0
  const startTime = timeSeries.find(Boolean) ?? null
  const endTime = [...timeSeries].reverse().find(Boolean) ?? null
  const coverage = buildMetricCoverage(metricSeriesMap)

  const parsedActivity = {
    file_name: fileName,
    file_format: fileFormat,
    metadata: {
      ...metadata,
      duration_seconds: roundValue(durationSeconds, 3) ?? 0,
      start_time: startTime,
      end_time: endTime,
      total_distance_m: roundValue(totalDistanceMeters, 3) ?? 0,
      sample_count: normalizedRawSamples.length,
      original_sample_count: rawSamples.length,
      inserted_idle_sample_count: gapDebug.inserted_sample_count,
    },
    metric_units: METRIC_UNITS,
    coverage,
    valid_attributes: validAttributes,
    extended_attributes: extendedAttributes,
    source_start_time: startTime,
    sample_elapsed_seconds: elapsedSeries,
    sample_distance_progress: buildProgressSeries(distanceSeries),
    frame_elapsed_seconds: [],
    frame_timestamps: [],
    frame_distance_progress: [],
    trim_start_seconds: 0,
    trim_end_seconds: roundValue(durationSeconds, 3) ?? 0,
    sample_course_points: courseSeries,
    sample_elevations: metricSeriesMap.elevation.series,
    course: courseSeries,
    elevation: metricSeriesMap.elevation.series,
    time: timeSeries,
    speed: metricSeriesMap.speed.series,
    heartrate: metricSeriesMap.heartrate.series,
    cadence: metricSeriesMap.cadence.series,
    power: metricSeriesMap.power.series,
    temperature: metricSeriesMap.temperature.series,
    gradient: metricSeriesMap.gradient.series,
    altitude: metricSeriesMap.altitude.series,
    air_pressure: metricSeriesMap.air_pressure.series,
    distance: metricSeriesMap.distance.series,
    g_force: metricSeriesMap.g_force.series,
    ground_contact_time: metricSeriesMap.ground_contact_time.series,
    heading: metricSeriesMap.heading.series,
    left_right_balance: metricSeriesMap.left_right_balance.series,
    pace: metricSeriesMap.pace.series,
    stroke_rate: metricSeriesMap.stroke_rate.series,
    stride_length: metricSeriesMap.stride_length.series,
    torque: metricSeriesMap.torque.series,
    vertical_oscillation: metricSeriesMap.vertical_oscillation.series,
    vertical_speed: metricSeriesMap.vertical_speed.series,
  }

  return {
    parsedActivity,
    debugPayload: {
      generated_at: new Date().toISOString(),
      file_name: fileName,
      file_format: fileFormat,
      idle_gap_fill: gapDebug,
      parsed_activity: parsedActivity,
    },
  }
}
